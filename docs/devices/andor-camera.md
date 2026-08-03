# Andor Cameras

Andor support is split by protocol family:

| Family | Device page | Current implementation |
| --- | --- | --- |
| SDK2 CCD/ICCD/EMCCD | [andor-sdk2.md](andor-sdk2.md) | USB discovery, firmware/runtime package checks, EP0 identity/status/FIFO/acquisition helpers, opt-in live bulk-IN `Mono16` capture, and vendor-runtime exposure/detector/cooler control |
| SDK3 sCMOS | [andor-sdk3.md](andor-sdk3.md) | USB discovery, hidden FX3 firmware initialization, confirmed EP0 status readbacks, firmware/runtime package checks, vendor-runtime feature control/readback, cooler control, and opt-in `Mono16` capture |

The Rust module remains `numanager_drivers::andor_camera` for compatibility.
Configured devices can use `driver = "andor_sdk2"` or `driver = "andor_sdk3"`;
`driver = "andor"` and `driver = "andor_camera"` remain accepted aliases.
