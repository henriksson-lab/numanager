# Third-Party Device Packages

This directory contains device-specific third-party data packages that are
excluded from this repository's MIT or Apache-2.0 license terms.

The general interim solution whenever firmware, a loader, or a vendor runtime
is required is to ship the original vendor package when redistribution terms
permit it, or load a user-configured local copy when they do not, behind an
optional backend until a project-owned firmware, loader, or open runtime
replacement exists. This is the default implementation path for every
firmware-dependent device, not a special-case exception or a reason to leave the
driver incomplete. The package may include original vendor firmware, loader, or
runtime files. Each package directory must record the file identity, SHA-256
digest, size, platform, and license boundary before the package is used by a
driver. Drivers should read or load package files only when an explicit
configured backend needs them.

Do not infer driver behavior from package presence alone. Commands, replies,
state transitions, and unit conversions still need evidence in device pages,
reverse notes, captured traces, hardware validation notes, or bench logs.

Audit exact: only when an explicit configured backend needs them.
