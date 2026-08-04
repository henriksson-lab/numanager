//! Windows USB access: which kernel driver owns a device node, and — only with
//! the user's explicit approval — binding the inbox WinUSB driver so that
//! userspace USB drivers can open the device.
//!
//! This is the Windows counterpart of the udev rules a host generates from
//! [`crate::usb_discovery::builtin_usb_vendor_claims`]: on either platform the
//! host must grant access to the node before a userspace driver can claim it.
//! On Windows "granting access" means a kernel driver binding, so it is a
//! system change, and this module never makes one on its own —
//! [`ensure_access`] is the only function that binds anything, no driver path
//! calls it, and it goes through an approval callback that is handed a warning
//! whenever an existing driver would be displaced.
//!
//! It must be WinUSB. `nusb`, the USB backend the drivers open through, calls
//! `winusb.dll`'s `WinUsb_Initialize` and rejects a node whose kernel service is
//! anything else — so libusbK is not an alternative binding, it is a broken one.
//!
//! Compiled unconditionally on Windows. Elsewhere it is behind the `winusb`
//! feature and builds against numanager-winusb's non-Windows entry points,
//! which report [`ErrorCode::Unsupported`](numanager_core::ErrorCode::Unsupported).

use numanager_core::Result;

pub use numanager_winusb::{InstallApproval, PortState, UsbFunction};

/// Which kernel driver owns the node for `function` right now.
///
/// Read-only. Errors when no matching device is present.
pub fn access_state(function: UsbFunction) -> Result<PortState> {
    numanager_winusb::port_state(&function)
}

/// Bind the inbox WinUSB driver to `function` so it can be opened from
/// userspace, if the user approves.
///
/// Returns `Ok(())` immediately when WinUSB is already bound. Otherwise
/// `approve` is called with an [`InstallApproval`] whose
/// [`prompt`](InstallApproval::prompt) is the text to show the user verbatim;
/// it opens with `WARNING:` whenever the binding would take the device away
/// from a driver that currently owns it, including the `usbccgp` parent of a
/// composite device. Declining returns
/// [`ErrorCode::Cancelled`](numanager_core::ErrorCode::Cancelled) and changes
/// nothing. The install itself requires an elevated process.
///
/// Intended for an explicit user-facing action ("set up this device"), not for
/// a driver's open path.
pub fn ensure_access(
    function: UsbFunction,
    approve: &dyn Fn(&InstallApproval) -> bool,
) -> Result<()> {
    numanager_winusb::ensure_winusb(&function, approve)
}

/// Why claiming `function` failed, when the reason is the Windows driver
/// binding — for appending to a driver's own error.
///
/// `None` when the binding is not the problem (WinUSB is already bound, no such
/// device is present, or this is not a Windows build), so a caller can add it
/// without pre-checking the platform.
pub fn claim_failure_hint(function: UsbFunction) -> Option<String> {
    // A composite device gives each function its own node, so the interface is
    // what to ask about; a single-function device has only the device node, and
    // an interface-qualified query matches nothing there. Try the narrower
    // question first and widen, keeping whichever function actually matched so
    // the diagnosis names the node the user has to act on.
    let device = UsbFunction {
        interface: None,
        ..function
    };
    let (function, state) = match access_state(function) {
        Ok(state) => (function, state),
        Err(_) if function.interface.is_some() => (device, access_state(device).ok()?),
        Err(_) => return None,
    };
    if state.is_winusb() {
        return None;
    }
    Some(format!(
        "{}. Bind WinUSB to it (numanager's WinUSB provisioning, run elevated, or Zadig); \
         libusbK is not an alternative, the USB backend requires WinUSB",
        state.diagnosis(&function)
    ))
}
