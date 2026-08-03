# USB3 Vision / GigE Vision / GenICam Protocol Evidence Note

Covers `numanager_drivers::usb3_vision`, `numanager_drivers::gige_vision`, and
`numanager_drivers::genicam` together, because all three are governed by the same
standards family and share the same remaining gaps.

## Status

| Field | Value |
| --- | --- |
| Plan target | Generic USB3 Vision and GigE Vision cameras driven through GenICam node maps |
| Current state | Live control paths exist on both transports: U3V `ReadMem`/`WriteMem` over a bulk command endpoint, GVCP register access over UDP. Stream leader/payload/trailer types are written but no receive path feeds them; frames come from local Netpbm fixtures. `genicam` parses real GenICam XML but is not bound to either transport |
| Better source status | **Public standards, freely or evaluation-freely available.** This is the top rung of the source ladder — no reverse engineering is required or appropriate for this target |
| Next evidence | A real USB3 Vision camera. The remaining unknowns are device-side facts (bootstrap register contents, vendor GenICam XML, stream framing in practice), not protocol unknowns |
| Evidence type | Public standard |
| Feasibility | Strong. Every remaining gap has an authoritative published answer |

Unlike the other notes in this directory, this target is **not** blocked on
evidence. It is blocked on implementation plus hardware validation.

## Standard Sources

| Document | Body | Access | Answers |
| --- | --- | --- | --- |
| USB3 Vision standard | A3 (formerly AIA) | Free for evaluation, but gated: web form, company-domain email required, link emailed after review. See below on licensing | U3V device layout, bootstrap register map (ABRM/SBRM/SIRM), stream leader/payload/trailer framing |
| GenICam GenCP | EMVA | Free, direct PDF, no registration | The U3V **control protocol**. `READMEM_CMD 0x0800` / `WRITEMEM_CMD 0x0802` and the command/ack framing already in `usb3_vision.rs` are GenCP, not U3V-specific |
| GenICam GenApi | EMVA | Free | Node-map semantics — `Integer`, `Enumeration`, `Converter`, `SwissKnife`, `pValue`, selectors. Governs `genicam.rs` |
| GenICam SFNC | EMVA | Free | Standard feature names. Replaces the hardcoded name→address bridges in `usb3_vision.rs` and `gige_vision.rs` |
| GenICam PFNC | EMVA | Free | Pixel format codes. Required to decode stream payloads into `PixelFormat` |
| GenICam GenTL | EMVA | Free | Only relevant if a vendor `.cti` producer is ever loaded as an optional backend; not needed for the native path |
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

numanager takes **no dependency** on any of these — the SDK-free policy stands.
They are reading references for disambiguating the specs.

| Project | Language | Covers | Useful for |
| --- | --- | --- | --- |
| `cameleon-rs/cameleon` | Rust | U3V + GenICam | Closest analogue to what numanager is building. `cameleon::u3v` is a low-level U3V API; the GenApi crate is a second opinion on node-map evaluation |
| `AravisProject/aravis` | C / GObject, **LGPL** | GigE Vision + U3V, mature | The most battle-tested reference for stream receive and reassembly |
| `ni/usb3vision` | C (Linux kernel driver) | U3V class devices | `u3v_shared.h` has the constants and bootstrap register layout in compact readable form |
| Wireshark `epan/dissectors/packet-u3v.c` | C | U3V | Spec-derived command and register tables; also lets you decode a real capture directly |

**Clean-room caution.** Per [`../protocol_evidence_plan.md`](../protocol_evidence_plan.md),
the specification is the primary source and should be cited as the evidence for
any behavior added. Aravis is LGPL; transliterating it into Rust would be a
derivation risk as well as a licence problem for an SDK-free MIT/Apache-style
crate. Use implementations to *disambiguate* spec wording, and record in the
device page which document — section and version — justified each behavior, not
which implementation was consulted.

## Current numanager State

| Area | File | State |
| --- | --- | --- |
| U3V control protocol | `crates/numanager-drivers/src/usb3_vision.rs` | Real. GenCP command codes, bootstrap register constants, packet encode, ACK validation against command id / request id / payload length |
| U3V USB I/O | same, behind `os-usb` | Real and live. `nusb` device open, interface claim, endpoint descriptor catalog, `bulk_out`/`bulk_in`. `ReadMem`/`WriteMem` back mapped property writes, trigger writes, and `RawRegisterAccess` |
| U3V stream framing types | same, `U3vStreamLeader`, `U3vStreamTrailer`, `U3vStreamPacket` | Types and match arms written; **not fed by any receive path** |
| GVCP control | `crates/numanager-drivers/src/gige_vision.rs` | Real and live. `UdpSocket`, port 3956, DISCOVERY/READREG/WRITEREG, ACK validation. Opt-in via `property.connect` + `property.camera_address` |
| GVSP reassembly | same, `GvspBlockReassembler` | Full logic — leader/payload/trailer, one-based packet-id validation, missing-packet tracking, completeness and length checks — and **never constructed anywhere in the repo**. Dead code awaiting a socket |
| GenICam node map | `crates/numanager-drivers/src/genicam.rs` | Real XML parser: `Integer`, `Float`, `Boolean`, `Enumeration`, `String`, `Command`, `IntSwissKnife`, `SwissKnife`, `Converter`, categories, ports, masked registers, selectors. Maintenance-node filtering enforced by the audit |
| GenICam ↔ transport binding | — | **Absent.** `GenicamTransport` has `Usb3Vision`/`GigeVision` variants but only local register backing is implemented. Each transport carries its own hardcoded ~10-name SFNC bridge instead |
| Frame source | all three | Local Netpbm PGM/PPM fixtures |
| Hardware validation | — | **None.** README support column is `-` for all three |

Note for anyone assuming otherwise: `toupcam` is **not** a USB3 Vision device
and shares no code with `usb3_vision`. It binds VIDs `0x0547`, `0x04b4`,
`0x232f` and replays a captured init sequence. Its `✓` validation status does
not transfer to this target.

## Gap Analysis

| Gap | Blocking evidence | Where the answer is |
| --- | --- | --- |
| U3V stream endpoint receive | None — implementation only | USB3 Vision spec (leader/trailer layout); PFNC for pixel codes; Aravis `arvuvstream.c` to disambiguate |
| GVSP receive path | None — implementation only. The reassembler already exists | GigE Vision spec; Aravis `arvgvstream.c` |
| Bind `genicam.rs` to U3V/GVCP register access | None — implementation only | GenCP (free) + GenApi (free) |
| Fetch device GenICam XML | **Hardware.** The XML lives on the camera, in the manifest table | U3V spec §manifest table; then feed to the existing parser |
| U3V active discovery | None — implementation only. Currently requires configured VID/PID | USB interface class/subclass/protocol triple, from the U3V spec; confirm against a real descriptor dump |
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
| 1 | `lsusb -v` on the camera. Record VID/PID, and the class/subclass/protocol triple on each interface | Whether the device is a conformant U3V composite device, and whether class-based discovery can replace configured VID/PID |
| 2 | With `os-usb`, `property.connect = true` and the configured VID/PID, issue `ReadMem` against the ABRM identity registers (Technology/Manufacturer Name/Model/Device Version/Serial) | Confirms the existing GenCP path works on real silicon — the single highest-value check, and it exercises code that is already written |
| 3 | Read the manifest table; retrieve the GenICam XML (may be Zip-compressed) and dump it to a file | Produces the first real vendor XML |
| 4 | Feed that XML to `genicam.rs`'s parser offline | Tests a 5,000-line parser against reality. Expect gaps — real vendor XML uses more node types than fixtures do |
| 5 | Read SBRM/SIRM: payload size, max leader/trailer size, required alignment, stream enable | The numbers needed to size buffers before any stream code is written |
| 6 | Enable the stream interface, queue a bulk IN, capture one leader + payload + trailer, feed the existing parsing types | Validates the framing types already in the tree |
| 7 | Cross-check against a vendor GenTL producer as an oracle — run it and diff register reads against numanager's | Distinguishes "our bug" from "camera quirk" without guessing |
| 8 | Repeat steps 1–3 on a second vendor's camera | Separates vendor-specific behavior from standard behavior before generalizing |

Step 7 is worth setting up early. A GenTL producer is a known-good implementation
of exactly the register access being brought up, and it turns ambiguous failures
into diffs.

## ZEISS Axiocam 105 R2 — A Concrete Candidate Device

| Field | Value |
| --- | --- |
| Device | ZEISS Axiocam 105 color R2 — an IDS-built USB3 Vision camera |
| Hardware ID | `USB\VID_1409&PID_8000&MI_00`; service-level `USB\VID_1409&PID_8000` |
| Identification caveat | `PID_8000` is the generic IDS U3V personality ID for the whole range. Identify by GenTL/GenICam device info (vendor, model, serial), never by PID |

Note the ZEISS producer speaks plain standard GenTL. Nothing about this camera
requires a ZEISS-specific code path — which is the argument for the native U3V
route over loading the `.cti`.

## Stop/Proceed Decision

| Decision | Condition |
| --- | --- |
| Proceed to SDK-free implementation | **Yes.** Standards are public and authoritative; two of numanager's three components already exist |
| Optional vendor-runtime backend | Not required. A GenTL producer is worth using as a bring-up oracle, not as a shipped backend |
| Evidence policy | Cite spec document, version and section for each added behavior. Do not claim hardware validation until a linked validation note exists — the README support column stays `-` until then |
| Order of work | Bind `genicam.rs` to the live transports first (needs only free EMVA documents and no hardware), then stream receive (needs the A3 document and a camera) |

## Implementation Gate

`usb3_vision` and `gige_vision` currently expose configured/fixture capture and
stream paths plus opt-in live register access. Until stream receive is
implemented and validated, capture and stream must continue to report their
fixture-backed source in metadata rather than implying live acquisition, and
the README support column for all three modules stays `-`.
