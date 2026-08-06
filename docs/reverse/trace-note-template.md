# Captured Trace Note Template

Use this template before promoting a captured protocol behavior into a driver.
Commit only curated summaries. Do not paste proprietary raw dumps, full SDK
logs, or large binary traces into the repository.

## Target And Status

| Field | Value |
| --- | --- |
| Target |  |
| Device page | `docs/devices/<target>.md` |
| Reverse note | `docs/reverse/<target>.md` |
| Status | promote / block / needs more trace |
| Driver behavior under consideration |  |

## Capture Identity

| Field | Value |
| --- | --- |
| Hardware identity | Model, serial/asset tag, firmware, modules, channels |
| Host identity | OS, driver stack, USB/serial adapter, VM/pass-through |
| Software identity | Name and version of the host software that drove the device; numanager commit |
| Config identity | Device configuration used, or the numanager discovery/config record |
| Capture tool | Tool/version/filter |
| Operator and date |  |
| Safety setup | Output limits, motion limits, interlocks, load/sample state |
| Clock alignment | How trace timestamps map to UI/runtime actions and printed output |

## Raw Evidence Storage

| Evidence item | Evidence class or package id | Integrity note retained externally? | Retention policy |
| --- | --- | --- | --- |
| Raw capture |  |  | local / lab storage / approved archive |
| Console/runtime output |  |  | local / lab storage / approved archive |
| Bench notes/photos/meters |  |  | local / lab storage / approved archive |

The raw capture and console/runtime output must cover the same action window.
For output-capable devices, also include hardware output/readback evidence for
that same window; otherwise the trace is not enough to validate the driver
claim.

Do not summarize the console output away. Include enough exact lines to check
device selection, requested action, typed units, command completion/readback,
error/fault behavior where applicable, and final disable/stop/abort state.

## Action Timeline

| Step | UI/runtime/API action | Trace timestamp or window | Command output | Hardware/bench output | Observed result |
| --- | --- | --- | --- | --- | --- |
| 1 |  |  |  |  |  |

## Transport Evidence

Fill only the sections that apply.

### Serial

| Item | Evidence |
| --- | --- |
| Port settings | Baud, parity, stop bits, flow control, timeout |
| Session setup | Open/reset/DTR/RTS behavior |
| Framing | Terminator, prefix/suffix, escaping, checksum |
| Reply model | ACK, status, data, and error layout |

### HID

| Item | Evidence |
| --- | --- |
| Identity | VID/PID, product, serial, report descriptor |
| Report layout | Report IDs, lengths, feature/output/input paths |
| Chunking | Payload chunk size, continuation, termination |
| Reply model | Status, data, error, timeout behavior |

### USB Vendor Or Bulk

| Item | Evidence |
| --- | --- |
| Identity | VID/PID, interfaces, endpoints, configuration |
| Open/init | Control or bulk transfers during initialization |
| Transfer layout | Request type, endpoint, payload fields |
| Reply model | Status, data, error, timeout behavior |

### Camera Frame Or Stream

| Item | Evidence |
| --- | --- |
| Platform route | V4L2/GStreamer/DirectShow/UVC/GenICam result |
| Control setup | Exposure, gain, ROI, binning, trigger, pixel format |
| Frame completion | Complete-frame signal, payload size, dimensions, stride |
| Buffering | Ring capacity, ownership, overflow and dropped-frame behavior |

## Decoded Command Candidates

| Action | Bytes/reports/transfers | Fields identified | Source class | Confidence | Driver decision |
| --- | --- | --- | --- | --- | --- |
|  |  | addressing / opcode / payload / unit / checksum / status | trace / manufacturer doc / open source / audited header | high / medium / low | promote / block / needs more trace |

## Completion And Fault Evidence

| Operation | Completion evidence | Printed/runtime output | Hardware output/readback | Fault or timeout behavior | Result |
| --- | --- | --- | --- | --- | --- |
|  |  |  |  |  | pass / fail / unknown |

## Promotion Decision

| Evidence item | Required update |
| --- | --- |
| Device page | Capability/property row and evidence gate |
| Evidence register | Support basis and missing evidence |
| Reverse note | Curated protocol finding and remaining unknowns |
| Driver code | Only the command behavior covered by this note |
| Examples/output | Only externally useful workflows with observable output/readback |
| Tests | No generated hardware-driver tests; link only explicitly requested validation checks |

## Remaining Questions

| Question | Evidence needed |
| --- | --- |
|  |  |
