# Photometrics PVCAM Runtime Package

This directory stores Teledyne Photometrics PVCAM vendor firmware/runtime
package files when a local installation or redistribution decision provides
them.

Files placed here are third-party data. They are not covered by this
repository's MIT or Apache-2.0 license terms. Record the file name, SHA-256 digest, platform, and license or redistribution
note in
a local manifest before enabling a PVCAM vendor-runtime backend.

The Rust driver may reference these files through config properties such as
`vendor_runtime_path` and `vendor_runtime_sha256`. With
`load_vendor_runtime=true`, the driver may attempt an explicit loadability probe
of the configured runtime after verifying the configured SHA-256. Camera
operations remain unsupported until the ABI binding and device behavior are
evidenced. The driver can also perform a digest-gated symbol-presence probe for
expected PVCAM exports when `load_vendor_runtime=true`; this loads the
configured runtime and checks symbol names without calling PVCAM functions or
treating symbol presence as proof of camera behavior.
