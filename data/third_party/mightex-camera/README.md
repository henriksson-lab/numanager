# Mightex Camera Runtime Package

This directory stores Mightex camera vendor firmware/runtime package files when
a local installation or redistribution decision provides them.

Files placed here are third-party data. They are not covered by this
repository's MIT or Apache-2.0 license terms. Record the file name, SHA-256 digest, platform, and license or redistribution
note in
a local manifest before enabling a Mightex camera vendor-runtime backend.

The Rust driver may reference these files through config properties such as
`vendor_runtime_path` and `vendor_runtime_sha256`. With
`load_vendor_runtime=true`, the driver may attempt an explicit loadability probe
of the configured runtime after verifying the configured SHA-256. Camera
one-shot capture may use the verified runtime through the documented buffered
camera SDK calls. The driver can also perform a digest-gated symbol-presence
probe for expected Mightex SDK exports such as `BUFCCDUSB_InitDevice`,
`BUFCCDUSB_StartCameraEngine`, `BUFCCDUSB_InstallFrameHooker`, and
`BUFCCDUSB_SetSoftTrigger`; this loads the configured runtime and checks symbol
names without calling Mightex functions or treating symbol presence as proof of
camera behavior.
