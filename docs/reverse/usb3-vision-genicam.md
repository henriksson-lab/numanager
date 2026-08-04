# USB3 Vision / GigE Vision / GenICam Protocol Evidence Note

Covers `numanager_drivers::usb3_vision`, `numanager_drivers::gige_vision`, and
`numanager_drivers::genicam` together: one standards family, one shared set of
gaps.

## Status

| Field | Value |
| --- | --- |
| Plan target | Generic USB3 Vision and GigE Vision cameras driven through GenICam node maps |
| Evidence class | **Public standards.** Top of the source ladder — no reverse engineering is required or appropriate for this target |
| Current state | Live control on both transports: U3V `ReadMem`/`WriteMem` over a bulk command endpoint, GVCP register access over UDP. Stream leader/payload/trailer types exist but no receive path feeds them; frames come from local Netpbm files. `genicam` parses real GenICam XML but is not bound to either transport |
| Hardware validation | **None.** The README support column is `-` for all three modules |
| Next evidence | A real USB3 Vision camera. The remaining unknowns are device-side facts — bootstrap register contents, the camera's own GenICam XML, stream framing in practice — not protocol unknowns |
| Feasibility | Strong. Every remaining gap has an authoritative published answer |

Unlike the other notes in this directory, this target is **not** blocked on
evidence. It is blocked on implementation plus hardware validation.

## Standard Sources

| Document | Body | Access | Answers |
| --- | --- | --- | --- |
| USB3 Vision standard | A3 (formerly AIA) | Free for evaluation, but gated: web form, company-domain email, link emailed after review | U3V device layout, bootstrap register map (ABRM/SBRM/SIRM), stream leader/payload/trailer framing |
| GenICam GenCP | EMVA | Free, direct PDF, no registration | The U3V **control protocol**. `READMEM_CMD 0x0800` / `WRITEMEM_CMD 0x0802` and the command/ack framing in `usb3_vision.rs` are GenCP, not U3V-specific |
| GenICam GenApi | EMVA | Free | Node-map semantics — `Integer`, `Enumeration`, `Converter`, `SwissKnife`, `pValue`, selectors. Governs `genicam.rs` |
| GenICam SFNC | EMVA | Free | Standard feature names. Replaces the hardcoded name→address bridges in `usb3_vision.rs` and `gige_vision.rs` |
| GenICam PFNC | EMVA | Free | Pixel format codes. Required to decode stream payloads into `PixelFormat` |
| GenICam GenTL | EMVA | Free | Only relevant if an optional transport-layer producer is ever loaded as a user-configured backend; not needed for the native path |
| GigE Vision standard | A3 | Same gated evaluation model as USB3 Vision | GVCP/GVSP specifics beyond what is already implemented |

Entry points:

- A3 USB3 Vision download — `https://www.automate.org/a3-content/usb3-vision-standard-download-standard-specification`
- A3 licence / product registration — `https://www.automate.org/vision/vision-standards/vision-standards-usb3-vision-license-product-registration`
- EMVA GenICam downloads — `https://www.emva.org/standards-technology/genicam/genicam-downloads/`
- GenCP 1.1 direct PDF — `https://www.emva.org/wp-content/uploads/GenCP_1.1.pdf`

Current GenICam package is 2025.10 (GenApi 3.5, GenTL 1.6, SFNC 2.7, PFNC 2.4,
GenCP 1.3.1, GenDC 1.1).

**The practical split:** the entire control-channel half is GenCP and is
downloadable with no gate at all. Only stream framing and the bootstrap register
map genuinely require the A3 document.

## Reference Implementations

numanager takes **no dependency** on any SDK or third-party implementation.
Open-source implementations of these same public standards may be read to
disambiguate spec wording, but the **specification is the citable evidence**:
record which document, version and section justifies each behavior, never which
implementation was consulted. Do not transliterate an LGPL implementation into
Rust — a derivation risk as well as a licence problem for an MIT/Apache-style
crate. See [`../protocol_evidence_plan.md`](../protocol_evidence_plan.md).

## Current numanager State

| Area | File | State |
| --- | --- | --- |
| U3V control protocol | `usb3_vision.rs` | Real. GenCP command codes, bootstrap register constants, packet encode, ACK validation against command id / request id / payload length |
| U3V USB I/O | same, behind `os-usb` | Real and live. Device open, interface claim, endpoint catalog, `bulk_out`/`bulk_in`. `ReadMem`/`WriteMem` back mapped property writes, trigger writes, and `RawRegisterAccess` |
| U3V stream framing types | same — `U3vStreamLeader`/`Trailer`/`Packet` | Types and match arms written; **not fed by any receive path** |
| GVCP control | `gige_vision.rs` | Real and live. UDP port 3956, DISCOVERY/READREG/WRITEREG, ACK validation. Opt-in via `property.connect` + `property.camera_address` |
| GVSP reassembly | same, `GvspBlockReassembler` | Full leader/payload/trailer logic with one-based packet-id validation, missing-packet tracking, completeness and length checks — and **never constructed anywhere in the repo**. Dead code awaiting a socket |
| GenICam node map | `genicam.rs` | Real XML parser: integer/float/bool/enum/string/command nodes, SwissKnife and Converter, categories, ports, masked registers, selectors. Maintenance-node filtering enforced by the audit |
| GenICam ↔ transport binding | — | **Absent.** `GenicamTransport` has `Usb3Vision`/`GigeVision` variants but only local register backing exists. Each transport carries its own hardcoded ~10-name SFNC bridge instead |
| Frame source | all three | Local Netpbm PGM/PPM files |

Note for anyone assuming otherwise: `toupcam` is **not** a USB3 Vision device
and shares no code with `usb3_vision`. It binds VIDs `0x0547`, `0x04b4`,
`0x232f` and replays a recorded init sequence. Its `✓` validation status does
not transfer to this target.

## Gap Analysis

| Gap | Blocking evidence | Where the answer is |
| --- | --- | --- |
| U3V stream endpoint receive | None — implementation only | USB3 Vision spec (leader/trailer layout); PFNC for pixel codes |
| GVSP receive path | None — implementation only; the reassembler already exists | GigE Vision spec |
| Bind `genicam.rs` to U3V/GVCP register access | None — implementation only | GenCP (free) + GenApi (free) |
| Fetch device GenICam XML | **Hardware.** The XML lives on the camera, in the manifest table | U3V spec §manifest table; then feed to the existing parser |
| U3V active discovery | None — implementation only; currently requires configured VID/PID | USB interface class/subclass/protocol triple, from the U3V spec; confirm against a real descriptor dump |
| GigE broadcast discovery | None — implementation only | GVCP DISCOVERY_CMD broadcast, already half-present |
| Pixel format mapping | None | PFNC 2.4 → numanager canonical `Mono8`/`Raw16`/`Rgb8` naming |
| Timestamps, trigger modes, packet resend | **Hardware** | Spec plus bench observation |

## Hardware Investigation Checklist

Run in order; each step is independently useful and the early ones need no code
changes. Record output per
[`../devices/hardware-validation-template.md`](../devices/hardware-validation-template.md)
and follow [`trace-capture-guide.md`](trace-capture-guide.md) §"USB Vendor Or
Bulk Targets" and §"Camera Frame Targets" for capture hygiene.

| Step | Action | Decides |
| --- | --- | --- |
| 1 | Dump the camera's USB descriptors. Record VID/PID and the class/subclass/protocol triple on each interface | Whether the device is a conformant U3V composite device, and whether class-based discovery can replace configured VID/PID |
| 2 | With `os-usb`, `property.connect = true` and the configured VID/PID, issue `ReadMem` against the ABRM identity registers (Technology / Manufacturer Name / Model / Device Version / Serial) | Confirms the existing GenCP path works on real silicon — the highest-value check, and it exercises code already written |
| 3 | Read the manifest table; retrieve the GenICam XML (may be Zip-compressed) and dump it to a file | Produces the first real camera-supplied XML |
| 4 | Feed that XML to the `genicam.rs` parser offline | Tests a 5,000-line parser against reality. Expect gaps — real camera XML uses more node types than local samples do |
| 5 | Read SBRM/SIRM: payload size, max leader/trailer size, required alignment, stream enable | The numbers needed to size buffers before any stream code is written |
| 6 | Enable the stream interface, queue a bulk IN, capture one leader + payload + trailer, feed the existing parsing types | Validates the framing types already in the tree |
| 7 | Diff register reads against a standard-conformant GenTL transport-layer producer used as a bring-up oracle | Distinguishes "our bug" from "camera quirk" without guessing. An oracle, not a source of protocol facts — behavior still gets cited to the spec |
| 8 | Repeat steps 1–3 on a second manufacturer's camera | Separates manufacturer-specific behavior from standard behavior before generalizing |

## ZEISS Axiocam 105 R2 — A Concrete Candidate Device

| Field | Value |
| --- | --- |
| Device | ZEISS Axiocam 105 color R2 — an IDS-built USB3 Vision camera |
| Hardware ID | `USB\VID_1409&PID_8000&MI_00`; service-level `USB\VID_1409&PID_8000` |
| Identification caveat | `PID_8000` is the generic IDS U3V personality ID for the whole range. Identify by GenICam device info (vendor, model, serial), never by PID |
| Code path | Standard U3V throughout. Nothing about this camera requires a manufacturer-specific path — which is the argument for the native U3V route |

## Stop/Proceed Decision

| Decision | Condition |
| --- | --- |
| Proceed to SDK-free implementation | **Yes.** Standards are public and authoritative; two of numanager's three components already exist |
| Optional vendor-runtime backend | Not required. A GenTL producer is worth using as a bring-up oracle, not as a shipped backend |
| Evidence policy | Cite spec document, version and section for each added behavior. Do not claim hardware validation until a linked validation note exists — the README support column stays `-` until then |
| Order of work | Bind `genicam.rs` to the live transports first (needs only free EMVA documents and no hardware), then stream receive (needs the A3 document and a camera) |

## Implementation Gate

`usb3_vision` and `gige_vision` currently expose configured/local-file capture
and stream paths plus opt-in live register access. Until stream receive is
implemented and validated, capture and stream must continue to report their
local-file source in metadata rather than implying live acquisition, and the
README support column for all three modules stays `-`.
