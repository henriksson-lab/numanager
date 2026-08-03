# Trace Capture Guide

This guide defines the evidence shape needed to unblock SDK-free drivers after
external protocol evidence has reached a transport boundary. It is not a test
plan for generated protocol fixtures. A trace is useful only when it can be tied to
real hardware, an exact command or UI action, and an observed hardware/result
state.

Use [`trace-note-template.md`](trace-note-template.md) for the curated note that
connects raw captures, runtime output, hardware output/readback, and the driver
behavior being promoted or left unexposed.

The raw transport capture is not independently testable unless the same note
also records the public command output. Treat stdout/stderr, runtime events,
completion/readback values, and the observed hardware output as first-class
capture artifacts for the same timestamped action window.

## Common Capture Header

Every trace note should start with this table or equivalent metadata.

| Field | Required value |
| --- | --- |
| Target device page | Link to `docs/devices/<target>.md` |
| Hardware identity | Model, serial/asset tag, firmware, module/channel inventory |
| Host identity | OS version, driver stack, USB/serial adapter, VM/pass-through if any |
| Software identity | Micro-Manager build, vendor software version, numanager commit if used |
| Config identity | Micro-Manager device config or numanager discovery/config record |
| Capture tool | Tool name/version and capture filter |
| Clock alignment | How trace timestamps map to user actions and command output |
| Safety setup | Output limits, interlocks, motion limits, emergency stop, sample/load state |

## Serial Targets

Use this for Okolab or any external-evidence target where the static pass shows COM/serial
transport.

| Step | Capture requirement |
| --- | --- |
| Open/session | Port name, baud rate, parity, stop bits, flow control, timeout, reset/DTR/RTS behavior |
| Discovery | Raw bytes for identity/module inventory request and reply |
| Readback | Raw request/reply for one stable read-only property with typed units |
| Safe write | Raw request/reply for one low-risk setpoint, later readback, and any busy/stable state |
| Error path | Invalid command, disconnected module, limit violation, or documented alarm reply |
| Framing | Start/end bytes, terminators, byte escaping, checksum bytes, retry behavior |

Promote a serial command only after the note identifies which bytes are
addressing, command, payload, checksum, and reply/status. Do not infer checksum
or ACK vocabulary from SDK return codes alone.

## HID Targets

Use this for Mightex BLS/SLC validation or any external-evidence target that proves HID
transport.

| Step | Capture requirement |
| --- | --- |
| Descriptor identity | VID/PID, product string, serial, report descriptor if available |
| Feature/output report | Report ID, report length, payload layout, command chunking |
| Input/status report | Report ID, report length, reply/status layout, termination rule |
| Discovery | Product/module/channel enumeration and fallback behavior |
| Safe output | Minimum safe output command, hold duration, readback/status, explicit disable |
| Fault path | Interlock/fault/error reply or invalid-command reply |

For Mightex BLS/SLC, the existing driver already records `last_command`,
`last_reply`, `last_outcome`, and reply report count. A useful bench note should
include that runtime output alongside HID report captures so command text and
report bytes can be audited together.

## USB Vendor Or Bulk Targets

Use this for Agilent Laser Combiner, MCL, ABS camera, and Mightex buffered
camera if they do not expose a platform-camera route.

| Step | Capture requirement |
| --- | --- |
| Descriptor identity | VID/PID, interfaces, endpoints, configuration, class/subclass/protocol |
| Open/init | Control/bulk transfers during library open and device initialization |
| Identity | Transfers for model/serial/firmware/module/channel discovery |
| Readback | Transfers for one read-only property or status query |
| Safe write/action | Transfers for a minimum safe move/output/control action plus later readback |
| Completion | Busy/status polling, completion transfer, timeout, or error transfer |
| Shutdown | Stop/abort/disable/close transfers and final safe hardware state |

For motion devices, include physical position before/after and whether the
controller reports busy, stopped, or fault. For output devices, include requested
output, hold duration, observable/readback state, and disable result.

## Camera Frame Targets

Use this for ABS camera and Mightex buffered camera unless the device is proven
compatible with a generic platform backend.

| Step | Capture requirement |
| --- | --- |
| Platform route check | Whether V4L2/GStreamer/DirectShow/UVC/GenICam can enumerate and capture frames |
| Control setup | Exposure, gain, ROI, binning, trigger, pixel format transfers and readbacks |
| Snap frame | Start command, frame payload, frame-complete status, dimensions, format, stride |
| Stream frame | Continuous transfer sequence with frame ordering, timestamps, and stop/abort |
| Buffer ownership | How SDK acquire/release maps to runtime frame-ready and retained frames |
| Backpressure | Ring capacity, overflow policy, dropped-frame counters, timeout/error behavior |

Do not claim working `CameraCapture` or `CameraStream` from only SDK-managed
buffer names. The trace must identify when a frame is complete, how payload
bytes map to pixels, and what happens when the consumer is slower than the
camera.

## Curation Rules

| Evidence | How to record it |
| --- | --- |
| Raw trace | Store locally or in approved artifact storage; do not commit proprietary dumps by default |
| Console/runtime output | Store or attach exact stdout/stderr or event logs for the same action window as the raw trace |
| Hardware output/readback | Record visible output, instrument readback, final safe state, and faults when applicable |
| Summary | Commit a curated summary in `docs/reverse/<target>.md` or a linked hardware note |
| Trace note | Use `docs/reverse/trace-note-template.md` when the captured action/output mapping is more than a few lines |
| Device page | Update the evidence gate and affected capability/property rows |
| Evidence register | Update `docs/devices/evidence.md` when support status changes |
| Driver code | Add only the command behavior covered by the curated evidence |
| Tests | Do not generate hardware-driver tests; keep assertions in evidence notes or an explicitly requested hardware-validation workflow |

If a trace confirms the target is SDK-only and no packet protocol can be
unsupported, record that as the support decision rather than writing a default
driver.
