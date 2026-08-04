# Protocol Evidence Notes

Interface facts and validation status per protocol target, used to decide
whether a target can advance to a driver implementation.

Most entries are evidence-limited. One is the inverse case: USB3 Vision / GigE
Vision / GenICam is governed by public standards, so it is blocked on
implementation and hardware validation rather than on evidence.

The current requirement-level status is summarized in
[`evidence-gate-audit.md`](evidence-gate-audit.md). The source policy and
clean-room criteria are in [`../protocol_evidence_plan.md`](../protocol_evidence_plan.md).

| Target | Note | Current disposition |
| --- | --- | --- |
| ABS Camera | [`abs-camera.md`](abs-camera.md) | Reverse engineered evidence exists; one-shot capture can use an optional vendor runtime, loaded only through explicit user configuration, with an explicit async software trigger; native transport, streaming, and broader controls are not exposed because USB protocol evidence is absent |
| Agilent Laser Combiner | [`agilent-laser-combiner.md`](agilent-laser-combiner.md), [`agilent-laser-combiner-protocol.md`](agilent-laser-combiner-protocol.md) | Transport grammar, full opcode table, and output units recorded from external evidence; hardware-support claims wait for a real board to confirm the handshake and the missing interlock/fault surface |
| MCL MicroDrive/NanoDrive | [`mcl.md`](mcl.md), [`mcl-protocol.md`](mcl-protocol.md) | Reverse engineered USB transport, endpoint map, VID/PID tables, vendor-request codes for both families, encoder format, and error mapping; typed motion/control is not exposed because payload fields, status-bit meaning, units/calibration, and move completion evidence is absent |
| Mightex / Mightex_BLS | [`mightex.md`](mightex.md) | BLS/SLC has reverse engineered HID output; camera one-shot capture and repeated one-shot stream can use an optional vendor runtime loaded only through explicit user configuration; native frame transport is not exposed because protocol evidence is absent |
| Okolab | [`okolab.md`](okolab.md) | Reverse engineered serial/configured runtime support exists from the frame grammar, checksum, handshake, error vocabulary, and command dictionary in [`okolab-protocol.md`](okolab-protocol.md); hardware-support claims wait for ACK/status/fault replies and matching runtime output/readback traces |
| Photometrics PVCAM | [`photometrics-pvcam.md`](photometrics-pvcam.md) | Configured evidence plus runtime-package file-status/digest/loadability/ABI-symbol checks, camera-name discovery, writable exposure setting, one-shot capture, repeated one-shot stream support, and runtime temperature read/setpoint control exist; native continuous streaming and broader control require further validation or validated native USB/PCIe traces |
| ToupTek USB cameras | [`toupcam-protocol.md`](toupcam-protocol.md), [`toupcam-model-registry.md`](toupcam-model-registry.md), [`toupcam-u3cmos03100kpa.md`](toupcam-u3cmos03100kpa.md) | Interface specification covering device shape, register access, sensor register map, exposure and gain arithmetic, streaming and frame framing, plus a 1337-variant camera catalogue. Models with a specified sensor register map are programmed directly; others fall back to a recorded open sequence. Open, streaming, capture, exposure and gain are hardware-validated on one model; the catalogue is corroborated on two |
| Squid controller | [`squid-protocol.md`](squid-protocol.md) | Open firmware/controller source protocol spec for the existing simulated protocol-backed fixture; hardware validation note pending |
| 3Z Optics IRIS | [`3z-optics-protocol.md`](3z-optics-protocol.md) | A Modbus-style serial register/coil map exists from an audited open adapter source; official product documentation confirms IRIS serial/TTL/controller operation, while the official register map and hardware validation are not yet recorded |
| USB3 Vision / GigE Vision / GenICam | [`usb3-vision-genicam.md`](usb3-vision-genicam.md) | Not evidence-blocked: the governing standards are public, and GenCP/GenApi/SFNC/PFNC are free from EMVA. Live U3V `ReadMem`/`WriteMem` and UDP GVCP register paths exist, stream framing types are written but unfed, and the GenICam node map is not yet bound to either transport; stream receive plus a real camera are what remain |
| WOSM MCU | [`wosm-protocol.md`](wosm-protocol.md) | Project-published v0.900 command page documents the current text-command surface for `dig_out`, `dig_in`, `dac_dest`, `stg_out_*`, macro timing, WML macro control, and controller-PC Telnet port `1023`; legacy sequence, blanking, pull-up, and raw analog-input commands remain separately source-backed |
| Xeryon ASCII and integrated CANopen stages | [`xeryon.md`](xeryon.md) | Manufacturer controller manuals document ASCII serial framing, command/readback tags, units, and status bits; Xeryon integrated-controller materials identify CANopen/CiA 402 and EDS/example paths; native ASCII support exists, and integrated CANopen support includes transaction planning, optional live SocketCAN/SLCAN NMT/SDO execution, and EDS object parsing |

These notes are the **specification side** and are kept clean-room. Record what
the interface is and what has been validated; state evidence by class
(manufacturer documentation, a public standard, open firmware or an audited open
adapter source, captured traffic from a physical device, a documented bench run).

Do not record how a fact was obtained from vendor software: no vendor binary
names, paths, versions or hashes, no addresses or symbol names taken from vendor
code, no analysis tooling or technique. Analysis records, tooling and raw
captures live outside this repository. Do not commit proprietary binaries or
large raw dumps.

When the static pass reaches a serial/HID/USB boundary, use
[`trace-capture-guide.md`](trace-capture-guide.md) to collect the hardware
identity, command/action mapping, completion, fault, and frame/stream evidence
needed before adding hardware operations that are not already defined by the
current reverse engineered support. Use [`trace-note-template.md`](trace-note-template.md)
when the capture needs a curated action timeline with runtime output and
hardware output/readback.
