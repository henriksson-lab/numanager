//! WinUSB provisioning — bind the inbox WinUSB driver to a USB device node so
//! numanager's userspace drivers (via `nusb`) can open it. This is the job a
//! user otherwise does by hand with Zadig.
//!
//! The crate splits into two layers:
//!
//! * **Detection** ([`port_state`]) — pure Rust over SetupAPI: which kernel
//!   driver, if any, currently owns the device node. This is safe, cheap, and
//!   available on its own; a driver can call it to tell the user "this device
//!   needs WinUSB" instead of just failing to open.
//! * **Install** ([`ensure_winusb`]) — generate a device-specific INF that binds
//!   the inbox `WinUSB.sys` (`Include = winusb.inf, Needs = WINUSB.NT`) and apply
//!   it to the node with `newdev`'s `UpdateDriverForPlugAndPlayDevices`.
//!
//! ## Install backend: native, not libwdi
//!
//! The original plan was to FFI into libwdi (Zadig's engine). That proved
//! intractable to build here: libwdi's install runs through an elevated
//! `installer.exe` that its `embedder` tool bakes into an auto-generated
//! `embedded.h`, alongside a hand-generated `config.h`/`build64.h` — a
//! multi-stage native pipeline that cannot be reproduced cleanly in a `cc`
//! build. So this crate installs natively instead: no C, no coinstaller (the
//! Win10+ inbox WinUSB needs none), no separate helper process — the calling
//! process must simply be elevated. libwdi's own source is vendored under
//! `third_party/libwdi/` as the reference for the one thing we do *not* yet do:
//! **self-signing the package** (see `pki.c`). Without a signed catalog the
//! install relies on the caller being elevated and on Windows' interactive
//! "unverified publisher" prompt; self-signing (to make it silent /
//! non-interactive) is the documented next step.
//!
//! **The install path compiles but has not been exercised on hardware** (it
//! needs a driverless device and Administrator rights); detection and the
//! approval gate *are* hardware-validated.
//!
//! ## WinUSB specifically, not "some generic driver"
//!
//! There is no libusbK/libusb0 alternative here, and that is not an omission:
//! `nusb` — the USB backend every numanager driver opens through — calls
//! `winusb.dll`'s `WinUsb_Initialize`, and refuses a node whose kernel service
//! is anything other than `winusb` (including `libusbK`). Binding libusbK with
//! Zadig makes a device *less* openable, not more.
//!
//! ## Composite devices
//!
//! On a multi-function device Windows binds `usbccgp` to the parent and gives
//! each USB function its own child node (`…&MI_00`); WinUSB binds to the child.
//! Identify the function with [`UsbFunction`] — the same VID/PID/interface a
//! driver already passes to `nusb` — rather than formatting device ids by hand.
//! Asking about the parent of such a device reports [`PortState::Composite`],
//! which says "re-ask about an interface", not "replace usbccgp".
//!
//! Everything here is Windows-only. On other platforms the entry points return
//! [`ErrorCode::Unsupported`] so callers can be written once and gated by result.

use numanager_core::{Error, ErrorCode, Result};
use std::fmt;

/// The USB function to provision, named the way a driver already knows it:
/// the VID/PID it probes for, plus the interface it claims.
///
/// [`hardware_id`](Self::hardware_id) renders the Windows device id, so callers
/// never format `USB\VID_…&PID_…&MI_…` strings themselves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UsbFunction {
    pub vendor_id: u16,
    pub product_id: u16,
    /// The interface (USB function) to bind, for a composite device. `None`
    /// addresses the device node as a whole, which is right for a
    /// single-function device.
    pub interface: Option<u8>,
}

impl UsbFunction {
    /// The device as a whole — no interface selected.
    pub const fn new(vendor_id: u16, product_id: u16) -> Self {
        Self {
            vendor_id,
            product_id,
            interface: None,
        }
    }

    /// Narrow to one USB function of a composite device.
    pub const fn interface(self, interface: u8) -> Self {
        Self {
            interface: Some(interface),
            ..self
        }
    }

    /// The Windows hardware id to match and to install against, e.g.
    /// `USB\VID_1234&PID_5678` or `USB\VID_1234&PID_5678&MI_00`. Both forms are
    /// among the ids Windows lists for the corresponding node, so the same
    /// string works for lookup and for `UpdateDriverForPlugAndPlayDevices`.
    pub fn hardware_id(&self) -> String {
        let Self {
            vendor_id,
            product_id,
            interface,
        } = self;
        match interface {
            Some(interface) => {
                format!("USB\\VID_{vendor_id:04X}&PID_{product_id:04X}&MI_{interface:02X}")
            }
            None => format!("USB\\VID_{vendor_id:04X}&PID_{product_id:04X}"),
        }
    }

    /// Whether one of a device node's hardware ids denotes this function.
    ///
    /// Matched component-wise rather than as a contiguous prefix, because
    /// Windows lists both `USB\VID_x&PID_y&MI_00` and the revision-qualified
    /// `USB\VID_x&PID_y&REV_0100&MI_00` for the same node. With no interface
    /// selected, a node carrying `&MI_` is rejected: that is a child function,
    /// not the device.
    pub fn matches_hardware_id(&self, hardware_id: &str) -> bool {
        let id = hardware_id.to_ascii_uppercase();
        let Self {
            vendor_id,
            product_id,
            interface,
        } = self;
        if !id.contains(&format!("VID_{vendor_id:04X}"))
            || !id.contains(&format!("PID_{product_id:04X}"))
        {
            return false;
        }
        match interface {
            Some(interface) => id.contains(&format!("&MI_{interface:02X}")),
            None => !id.contains("&MI_"),
        }
    }
}

impl fmt::Display for UsbFunction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self {
            vendor_id,
            product_id,
            interface,
        } = self;
        write!(f, "USB {vendor_id:04x}:{product_id:04x}")?;
        if let Some(interface) = interface {
            write!(f, " interface {interface}")?;
        }
        Ok(())
    }
}

#[cfg(windows)]
mod signing;

/// The kernel driver currently bound to a USB device node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortState {
    /// No function driver is bound (e.g. Windows problem code 28, "drivers not
    /// installed"). Binding WinUSB here is a clean install that displaces
    /// nothing.
    Free,
    /// WinUSB is already bound. [`ensure_winusb`] treats this as success.
    WinUsb,
    /// The node is the parent of a composite (multi-function) device, owned by
    /// Windows' USB generic parent driver `usbccgp`. That binding is correct
    /// and normally must stay: each USB function has its own child node, and
    /// WinUSB binds there. Re-query with [`UsbFunction::interface`] set to the
    /// function that carries the endpoints the driver needs.
    Composite,
    /// Another driver owns the node (its service name, e.g. a vendor `.sys`).
    /// Installing WinUSB would replace a working driver, so callers must warn
    /// before proceeding.
    TakenBy(String),
}

impl PortState {
    /// Whether WinUSB is already bound.
    pub fn is_winusb(&self) -> bool {
        matches!(self, PortState::WinUsb)
    }

    /// Whether installing WinUSB would displace an existing, working driver.
    /// Callers must surface a warning to the user when this is true.
    pub fn would_displace(&self) -> bool {
        matches!(self, PortState::Composite | PortState::TakenBy(_))
    }

    /// The kernel service that owns the node today, for message text.
    pub fn owner(&self) -> &str {
        match self {
            PortState::Free => "no driver",
            PortState::WinUsb => "WinUSB",
            PortState::Composite => "usbccgp (USB generic parent)",
            PortState::TakenBy(service) => service,
        }
    }

    /// Why opening `function` fails today and what the user can do about it —
    /// for error messages. Describes; installs nothing.
    pub fn diagnosis(&self, function: &UsbFunction) -> String {
        let id = function.hardware_id();
        match self {
            PortState::Free => format!(
                "{function} ({id}) has no kernel driver bound; it needs the inbox WinUSB driver \
                 before it can be opened from userspace"
            ),
            PortState::WinUsb => {
                format!("{function} ({id}) is already bound to WinUSB")
            }
            PortState::Composite => format!(
                "{function} ({id}) is a composite device whose parent is owned by usbccgp; WinUSB \
                 binds per function, so the interface node (…&MI_xx) is what needs it"
            ),
            PortState::TakenBy(service) => format!(
                "{function} ({id}) is owned by the '{service}' driver, not WinUSB; it cannot be \
                 opened from userspace until WinUSB is bound instead, which would take the device \
                 away from '{service}'"
            ),
        }
    }
}

/// What a caller must put in front of the user before [`ensure_winusb`]
/// touches anything.
///
/// [`prompt`](Self::prompt) is pre-composed so every caller warns consistently;
/// it opens with `WARNING:` exactly when [`PortState::would_displace`] holds.
/// A caller that shows nothing else must still show that.
#[derive(Debug, Clone)]
pub struct InstallApproval<'a> {
    /// The function that would be bound.
    pub function: &'a UsbFunction,
    /// Its Windows hardware id — what the generated INF matches on.
    pub hardware_id: String,
    /// What owns the node right now.
    pub state: &'a PortState,
    /// The message to show the user verbatim.
    pub prompt: String,
}

impl<'a> InstallApproval<'a> {
    fn new(function: &'a UsbFunction, state: &'a PortState) -> Self {
        let hardware_id = function.hardware_id();
        let prompt = match state {
            PortState::Free | PortState::WinUsb => format!(
                "Bind the inbox WinUSB driver to {function} ({hardware_id})?\n\
                 No driver owns this device, so nothing is displaced. This changes Windows \
                 driver bindings for the device and requires an Administrator process."
            ),
            PortState::Composite => format!(
                "WARNING: this REPLACES a working driver.\n\
                 {function} ({hardware_id}) is a composite device currently owned by usbccgp, the \
                 Windows USB generic parent driver. Binding WinUSB here detaches every function of \
                 the device — all of its child interfaces disappear, and any software using any of \
                 them stops working.\n\
                 The usual fix is to bind WinUSB to one interface (…&MI_xx) instead, leaving \
                 usbccgp on the parent.\n\
                 Proceed anyway?"
            ),
            PortState::TakenBy(service) => format!(
                "WARNING: this REPLACES a working driver.\n\
                 {function} ({hardware_id}) is currently controlled by '{service}'. Binding WinUSB \
                 detaches '{service}' from the device: any vendor software that talks to it \
                 through '{service}' will stop working until you restore that driver by hand \
                 (Device Manager > the device > Update driver > Browse > Let me pick).\n\
                 Proceed anyway?"
            ),
        };
        Self {
            function,
            hardware_id,
            state,
            prompt,
        }
    }
}

/// Report which driver owns the node for `function`.
///
/// This is the entry point drivers should use: it takes the VID/PID (and, for a
/// composite device, the interface) a driver already has, and never asks the
/// caller to format a Windows device id.
///
/// Errors if no matching device is present, or on a SetupAPI failure.
pub fn port_state(function: &UsbFunction) -> Result<PortState> {
    port_state_matching(
        &|id| function.matches_hardware_id(id),
        &function.to_string(),
    )
}

/// Report which driver owns the first present USB device whose hardware id
/// contains `hardware_id_prefix` (case-insensitive), e.g.
/// `"USB\\VID_5354&PID_009A"`.
///
/// The escape hatch for ids [`UsbFunction`] cannot express — a container id, or
/// a device matched on something other than VID/PID. Note that a bare
/// VID/PID prefix also matches the `…&MI_xx` children of a composite device, so
/// which node is reported is whichever the enumeration reaches first; prefer
/// [`port_state`] when the target is a VID/PID.
pub fn port_state_by_hardware_id(hardware_id_prefix: &str) -> Result<PortState> {
    let target = hardware_id_prefix.to_ascii_uppercase();
    port_state_matching(
        &|id| id.to_ascii_uppercase().contains(&target),
        hardware_id_prefix,
    )
}

#[cfg(windows)]
fn port_state_matching(matches: &dyn Fn(&str) -> bool, description: &str) -> Result<PortState> {
    win::port_state(matches, description)
}

/// Non-Windows: WinUSB provisioning does not apply.
#[cfg(not(windows))]
fn port_state_matching(_matches: &dyn Fn(&str) -> bool, _description: &str) -> Result<PortState> {
    Err(Error::new(
        ErrorCode::Unsupported,
        "WinUSB provisioning is only available on Windows",
    ))
}

/// Ensure WinUSB is bound to `function`, with the user's consent.
///
/// * If WinUSB is already bound, returns `Ok(())` without calling `approve`
///   (idempotent).
/// * Otherwise `approve` is called with an [`InstallApproval`] carrying the
///   current [`PortState`] and a ready-composed [`prompt`](InstallApproval::prompt)
///   that opens with `WARNING:` whenever a working driver would be displaced.
///   The caller must show it — this function never installs unprompted. If
///   `approve` returns `false`, this returns [`ErrorCode::Cancelled`] and
///   touches nothing.
/// * On approval, the install runs. It requires elevation; on a non-elevated
///   process the backend surfaces that as an error rather than silently failing.
pub fn ensure_winusb(
    function: &UsbFunction,
    approve: &dyn Fn(&InstallApproval) -> bool,
) -> Result<()> {
    let state = port_state(function)?;
    if state.is_winusb() {
        return Ok(());
    }
    let approval = InstallApproval::new(function, &state);
    if !approve(&approval) {
        return Err(Error::new(
            ErrorCode::Cancelled,
            format!(
                "WinUSB installation for {function} was not approved; the device is still owned by {}",
                state.owner()
            ),
        ));
    }
    install_winusb(&approval.hardware_id)
}

/// Perform the install: require elevation, write a WinUSB INF, sign the package,
/// and apply it. See the module docs for why the install is native rather than
/// libwdi; signing is a faithful port of libwdi's `pki.c` (see [`signing`]).
#[cfg(windows)]
fn install_winusb(hardware_id: &str) -> Result<()> {
    win::install(hardware_id)
}

#[cfg(not(windows))]
fn install_winusb(_hardware_id: &str) -> Result<()> {
    Err(Error::new(
        ErrorCode::Unsupported,
        "WinUSB provisioning is only available on Windows",
    ))
}

/// Revoke the trust that [`ensure_winusb`] established: delete numanager's
/// self-signed code-signing certificate from the LocalMachine `Root` and
/// `TrustedPublisher` stores.
///
/// Signing an install adds a self-signed cert as a machine trust anchor (its
/// private key is destroyed immediately, but the public cert stays trusted).
/// Call this to remove it — e.g. from an uninstaller or a `gel` maintenance
/// command. Requires an elevated process; a no-op if the cert isn't present.
#[cfg(windows)]
pub fn remove_signing_cert() -> Result<()> {
    signing::remove_cert_from_store(signing::CERT_SUBJECT, "Root")?;
    signing::remove_cert_from_store(signing::CERT_SUBJECT, "TrustedPublisher")?;
    Ok(())
}

/// Non-Windows stub: nothing was ever installed to remove.
#[cfg(not(windows))]
pub fn remove_signing_cert() -> Result<()> {
    Err(Error::new(
        ErrorCode::Unsupported,
        "WinUSB provisioning is only available on Windows",
    ))
}

/// One driver package that [`ensure_winusb`] published into the Windows driver
/// store.
///
/// `published_name` is the store's own `oemNN.inf` handle — the name every
/// removal API wants. `original_name` is what the package was called when it
/// was submitted, which is how these are told apart from WinUSB packages that
/// other tools (Zadig, libwdi, a vendor installer) may have installed.
#[cfg(feature = "uninstall")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledPackage {
    pub published_name: String,
    pub original_name: String,
}

/// Every WinUSB package this crate published, newest first.
///
/// Deliberately scoped to numanager's own packages. A device can be bound to
/// WinUSB by any number of tools, and removing a package this crate did not
/// install would silently break unrelated hardware.
#[cfg(all(windows, feature = "uninstall"))]
pub fn installed_packages() -> Result<Vec<InstalledPackage>> {
    win::enum_installed_packages()
}

#[cfg(all(not(windows), feature = "uninstall"))]
pub fn installed_packages() -> Result<Vec<InstalledPackage>> {
    Err(Error::new(
        ErrorCode::Unsupported,
        "WinUSB provisioning is only available on Windows",
    ))
}

/// Detach every device node one of our packages bound, **including nodes that
/// are not currently plugged in**, so each re-enumerates with no stored
/// binding.
///
/// Removing the package alone is not enough: Windows keeps a node's binding in
/// the `Enum` tree while the device is absent, so a package-only uninstall
/// leaves exactly the devices a user is most likely to be cleaning up still
/// pointing at WinUSB. Returns the instance ids detached.
///
/// Must run **before** [`remove_installed_packages`] — nodes are matched on the
/// `oemNN.inf` that bound them, and deleting the package destroys that link.
#[cfg(all(windows, feature = "uninstall"))]
pub fn remove_bound_nodes() -> Result<Vec<String>> {
    let packages = win::enum_installed_packages()?;
    let nodes = win::nodes_bound_by(&packages)?;
    let mut removed = Vec::new();
    for node in nodes {
        win::remove_node(&node)?;
        removed.push(node);
    }
    Ok(removed)
}

#[cfg(all(not(windows), feature = "uninstall"))]
pub fn remove_bound_nodes() -> Result<Vec<String>> {
    Err(Error::new(
        ErrorCode::Unsupported,
        "WinUSB provisioning is only available on Windows",
    ))
}

/// Delete every package from [`installed_packages`], unbinding any device still
/// attached so it falls back to whatever driver PnP picks next.
///
/// Returns the packages actually removed; an empty vector means nothing of
/// numanager's was installed. Requires elevation.
///
/// A full uninstall is three steps in order: [`remove_bound_nodes`], then this,
/// then [`remove_signing_cert`]. Nodes first because they are matched via the
/// package; the certificate last so a failed package removal never leaves the
/// store trusting nothing for packages that are still installed.
#[cfg(all(windows, feature = "uninstall"))]
pub fn remove_installed_packages() -> Result<Vec<InstalledPackage>> {
    win::remove_installed_packages()
}

#[cfg(all(not(windows), feature = "uninstall"))]
pub fn remove_installed_packages() -> Result<Vec<InstalledPackage>> {
    Err(Error::new(
        ErrorCode::Unsupported,
        "WinUSB provisioning is only available on Windows",
    ))
}

#[cfg(windows)]
mod win {
    use super::{Error, ErrorCode, PortState, Result};
    use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
        SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo, SetupDiGetClassDevsW,
        SetupDiGetDeviceRegistryPropertyW, UpdateDriverForPlugAndPlayDevicesW, DIGCF_ALLCLASSES,
        DIGCF_PRESENT, HDEVINFO, INSTALLFLAG_FORCE, SETUP_DI_REGISTRY_PROPERTY, SPDRP_HARDWAREID,
        SPDRP_SERVICE, SP_DEVINFO_DATA,
    };
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE};

    // Only the node-removal path needs device properties and instance ids.
    #[cfg(feature = "uninstall")]
    use windows_sys::core::GUID;
    #[cfg(feature = "uninstall")]
    use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
        SetupDiGetDeviceInstanceIdW, SetupDiGetDevicePropertyW,
    };
    #[cfg(feature = "uninstall")]
    use windows_sys::Win32::Foundation::DEVPROPKEY;
    use windows_sys::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    /// `SetupDiGetClassDevsW` returns `INVALID_HANDLE_VALUE` (-1 as a handle) on
    /// failure; `HDEVINFO` is an `isize`, so compare against that.
    const INVALID_DEVINFO: HDEVINFO = -1;

    pub(super) fn port_state(
        matches: &dyn Fn(&str) -> bool,
        description: &str,
    ) -> Result<PortState> {
        let enumerator = wide("USB");
        // SAFETY: FFI. The device info set is destroyed before returning, and no
        // borrowed pointer outlives the call it is passed to.
        unsafe {
            let set = SetupDiGetClassDevsW(
                core::ptr::null(),
                enumerator.as_ptr(),
                core::ptr::null_mut(),
                DIGCF_PRESENT | DIGCF_ALLCLASSES,
            );
            if set == INVALID_DEVINFO {
                return Err(last_err("SetupDiGetClassDevsW"));
            }

            let mut found: Option<PortState> = None;
            let mut index = 0u32;
            loop {
                let mut data: SP_DEVINFO_DATA = core::mem::zeroed();
                data.cbSize = core::mem::size_of::<SP_DEVINFO_DATA>() as u32;
                if SetupDiEnumDeviceInfo(set, index, &mut data) == 0 {
                    break; // ERROR_NO_MORE_ITEMS
                }
                index += 1;

                let Some(hwids) = get_reg_prop(set, &data, SPDRP_HARDWAREID) else {
                    continue;
                };
                if !decode_multi_sz(&hwids).iter().any(|h| matches(h)) {
                    continue;
                }

                // A driverless node has no SPDRP_SERVICE value; treat its
                // absence as "free".
                let service = get_reg_prop(set, &data, SPDRP_SERVICE)
                    .map(|b| decode_sz(&b))
                    .unwrap_or_default();
                found = Some(if service.is_empty() {
                    PortState::Free
                } else if service.eq_ignore_ascii_case("WinUSB") {
                    PortState::WinUsb
                } else if service.eq_ignore_ascii_case("usbccgp") {
                    PortState::Composite
                } else {
                    PortState::TakenBy(service)
                });
                break;
            }

            SetupDiDestroyDeviceInfoList(set);
            found.ok_or_else(|| {
                Error::new(
                    ErrorCode::Transport,
                    format!("no present USB device matches {description}"),
                )
            })
        }
    }

    /// Read a device-registry property as raw bytes: size probe, then fetch.
    /// Returns `None` if the property is absent (a driverless node has no
    /// service) or on any failure.
    ///
    /// # Safety
    /// `set` must be a live device info set and `data` an entry enumerated from
    /// it.
    unsafe fn get_reg_prop(
        set: HDEVINFO,
        data: *const SP_DEVINFO_DATA,
        prop: SETUP_DI_REGISTRY_PROPERTY,
    ) -> Option<Vec<u8>> {
        let mut needed = 0u32;
        SetupDiGetDeviceRegistryPropertyW(
            set,
            data,
            prop,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            0,
            &mut needed,
        );
        if needed == 0 {
            return None;
        }
        let mut buf = vec![0u8; needed as usize];
        let ok = SetupDiGetDeviceRegistryPropertyW(
            set,
            data,
            prop,
            core::ptr::null_mut(),
            buf.as_mut_ptr(),
            needed,
            &mut needed,
        );
        if ok == 0 {
            return None;
        }
        buf.truncate(needed as usize);
        Some(buf)
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(core::iter::once(0)).collect()
    }

    fn to_u16s(bytes: &[u8]) -> Vec<u16> {
        bytes
            .chunks_exact(2)
            .map(|c| u16::from_ne_bytes([c[0], c[1]]))
            .collect()
    }

    /// Decode a REG_SZ (single NUL-terminated wide string).
    fn decode_sz(bytes: &[u8]) -> String {
        let u16s = to_u16s(bytes);
        let end = u16s.iter().position(|&c| c == 0).unwrap_or(u16s.len());
        String::from_utf16_lossy(&u16s[..end])
    }

    /// Decode a REG_MULTI_SZ (NUL-separated wide strings, double-NUL terminated).
    fn decode_multi_sz(bytes: &[u8]) -> Vec<String> {
        to_u16s(bytes)
            .split(|&c| c == 0)
            .filter(|s| !s.is_empty())
            .map(String::from_utf16_lossy)
            .collect()
    }

    fn last_err(ctx: &str) -> Error {
        let code = unsafe { GetLastError() };
        Error::new(
            ErrorCode::Transport,
            format!("{ctx} failed (GetLastError=0x{code:08x})"),
        )
    }

    // ------------------------------------------------------------------ install

    /// Bind the inbox WinUSB driver to the device whose hardware id is
    /// `hardware_id` (e.g. `USB\VID_5354&PID_009A`). See the module docs: this is
    /// native (no libwdi), unsigned (relies on elevation + Windows' interactive
    /// publisher warning), and not yet hardware-tested.
    pub(super) fn install(hardware_id: &str) -> Result<()> {
        require_elevated()?;
        let dir = make_temp_dir()?;
        let result = (|| {
            let inf = write_winusb_inf(&dir, hardware_id)?;
            // Sign the package so the install is silent (no "unverified
            // publisher" prompt): build the catalog over the INF, then
            // self-sign it and trust the cert. See `signing`.
            let cat = dir.join(CAT_FILENAME);
            crate::signing::create_cat(&cat, hardware_id, &dir, INF_FILENAME)?;
            crate::signing::self_sign_file(&cat)?;
            update_driver(hardware_id, &inf)
        })();
        // The INF/CAT are copied into the driver store by the install, so the
        // temp copies are disposable. Best-effort cleanup.
        let _ = std::fs::remove_dir_all(&dir);
        result
    }

    /// File names for the generated package. The `.cat` base name must match the
    /// INF's `CatalogFile=` directive.
    const INF_FILENAME: &str = "numanager_winusb.inf";
    const CAT_FILENAME: &str = "numanager_winusb.cat";

    /// Read the driver store's package list.
    ///
    /// `pnputil`'s field *labels* are localized but its *values* are not, so the
    /// parse keys on values only: a block mentioning [`INF_FILENAME`] is one of
    /// ours, and the `oemNN.inf` token in that block is its published name.
    #[cfg(feature = "uninstall")]
    pub(super) fn enum_installed_packages() -> Result<Vec<super::InstalledPackage>> {
        let output = std::process::Command::new("pnputil.exe")
            .arg("/enum-drivers")
            .output()
            .map_err(|error| {
                Error::new(
                    ErrorCode::Driver,
                    format!("running `pnputil /enum-drivers` failed: {error}"),
                )
            })?;
        let text = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
        let mut found = Vec::new();
        for block in text.split("\n\n") {
            if !block.contains(INF_FILENAME) {
                continue;
            }
            let published = block.split_whitespace().find(|token| {
                let lower = token.to_ascii_lowercase();
                lower.starts_with("oem") && lower.ends_with(".inf")
            });
            if let Some(published) = published {
                found.push(super::InstalledPackage {
                    published_name: published.to_string(),
                    original_name: INF_FILENAME.to_string(),
                });
            }
        }
        Ok(found)
    }

    /// Delete each of our packages, unbinding any device still using it.
    ///
    /// `/uninstall` is what detaches bound devices; without it the package is
    /// merely removed from the store and the devices keep running the copy they
    /// already have. Errors abort the sweep rather than continuing, so a
    /// partial removal is reported rather than silently completed.
    /// Instance ids of every device node bound by one of our packages,
    /// **including nodes that are not currently plugged in**.
    ///
    /// `DIGCF_PRESENT` is deliberately omitted: a node keeps its stored binding
    /// while absent, so an uninstall that only touched present devices would
    /// silently leave the very devices a user is most likely to be cleaning up.
    /// Nodes are matched on `DEVPKEY_Device_DriverInfPath`, which names the
    /// `oemNN.inf` that bound them — the only link back to us, and one that
    /// disappears once the package is deleted, so this must run first.
    #[cfg(feature = "uninstall")]
    pub(super) fn nodes_bound_by(packages: &[super::InstalledPackage]) -> Result<Vec<String>> {
        // DEVPKEY_Device_DriverInfPath — {a8b865dd-2e3d-4094-ad97-e593a70c75d6}, pid 5.
        const DEVPKEY_DEVICE_DRIVER_INF_PATH: DEVPROPKEY = DEVPROPKEY {
            fmtid: GUID {
                data1: 0xa8b8_65dd,
                data2: 0x2e3d,
                data3: 0x4094,
                data4: [0xad, 0x97, 0xe5, 0x93, 0xa7, 0x0c, 0x75, 0xd6],
            },
            pid: 5,
        };

        let wanted: Vec<String> = packages
            .iter()
            .map(|package| package.published_name.to_ascii_lowercase())
            .collect();
        let mut found = Vec::new();

        // SAFETY: FFI. The device info set is destroyed on every exit path;
        // buffers are sized from the required-size query before the real read.
        unsafe {
            let set = SetupDiGetClassDevsW(
                core::ptr::null(),
                core::ptr::null(),
                core::ptr::null_mut(),
                DIGCF_ALLCLASSES,
            );
            if set as isize == -1 {
                return Err(last_err("SetupDiGetClassDevsW"));
            }
            let mut index = 0u32;
            loop {
                let mut data = SP_DEVINFO_DATA {
                    cbSize: core::mem::size_of::<SP_DEVINFO_DATA>() as u32,
                    ..core::mem::zeroed()
                };
                if SetupDiEnumDeviceInfo(set, index, &mut data) == 0 {
                    break;
                }
                index += 1;

                let mut kind = 0u32;
                let mut needed = 0u32;
                SetupDiGetDevicePropertyW(
                    set,
                    &data,
                    &DEVPKEY_DEVICE_DRIVER_INF_PATH,
                    &mut kind,
                    core::ptr::null_mut(),
                    0,
                    &mut needed,
                    0,
                );
                if needed == 0 {
                    continue;
                }
                let mut buf = vec![0u8; needed as usize];
                if SetupDiGetDevicePropertyW(
                    set,
                    &data,
                    &DEVPKEY_DEVICE_DRIVER_INF_PATH,
                    &mut kind,
                    buf.as_mut_ptr(),
                    needed,
                    &mut needed,
                    0,
                ) == 0
                {
                    continue;
                }
                let inf = decode_sz(&buf).to_ascii_lowercase();
                if !wanted.iter().any(|name| *name == inf) {
                    continue;
                }

                let mut id_len = 0u32;
                SetupDiGetDeviceInstanceIdW(set, &data, core::ptr::null_mut(), 0, &mut id_len);
                if id_len == 0 {
                    continue;
                }
                let mut id = vec![0u16; id_len as usize];
                if SetupDiGetDeviceInstanceIdW(set, &data, id.as_mut_ptr(), id_len, &mut id_len)
                    != 0
                {
                    let text = String::from_utf16_lossy(&id);
                    found.push(text.trim_end_matches('\0').to_string());
                }
            }
            SetupDiDestroyDeviceInfoList(set);
        }
        Ok(found)
    }

    /// Detach a device node so it re-enumerates with no stored binding. Works
    /// on absent nodes: `pnputil` addresses the `Enum` entry, not a live device.
    #[cfg(feature = "uninstall")]
    pub(super) fn remove_node(instance_id: &str) -> Result<()> {
        let output = std::process::Command::new("pnputil.exe")
            .args(["/remove-device", instance_id])
            .output()
            .map_err(|error| {
                Error::new(
                    ErrorCode::Driver,
                    format!("running `pnputil /remove-device {instance_id}` failed: {error}"),
                )
            })?;
        if !output.status.success() {
            return Err(Error::new(
                ErrorCode::Driver,
                format!(
                    "removing device node {instance_id} failed ({}): {}",
                    output.status,
                    String::from_utf8_lossy(&output.stdout).trim()
                ),
            ));
        }
        Ok(())
    }

    #[cfg(feature = "uninstall")]
    pub(super) fn remove_installed_packages() -> Result<Vec<super::InstalledPackage>> {
        require_elevated()?;
        let mut removed = Vec::new();
        for package in enum_installed_packages()? {
            let output = std::process::Command::new("pnputil.exe")
                .args([
                    "/delete-driver",
                    &package.published_name,
                    "/uninstall",
                    "/force",
                ])
                .output()
                .map_err(|error| {
                    Error::new(
                        ErrorCode::Driver,
                        format!(
                            "running `pnputil /delete-driver {}` failed: {error}",
                            package.published_name
                        ),
                    )
                })?;
            if !output.status.success() {
                return Err(Error::new(
                    ErrorCode::Driver,
                    format!(
                        "removing {} failed ({}); removed {} package(s) first: {}",
                        package.published_name,
                        output.status,
                        removed.len(),
                        String::from_utf8_lossy(&output.stdout).trim()
                    ),
                ));
            }
            removed.push(package);
        }
        Ok(removed)
    }

    /// Fail early with a clear message if the process is not elevated — the
    /// install APIs need Administrator rights and would otherwise fail deep in
    /// SetupAPI with an opaque error.
    fn require_elevated() -> Result<()> {
        // SAFETY: FFI. `token` is closed before returning; `info` is a local of
        // the exact size passed.
        unsafe {
            let mut token: HANDLE = core::ptr::null_mut();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
                return Err(last_err("OpenProcessToken"));
            }
            let mut info = TOKEN_ELEVATION { TokenIsElevated: 0 };
            let mut ret_len = 0u32;
            let ok = GetTokenInformation(
                token,
                TokenElevation,
                (&mut info as *mut TOKEN_ELEVATION).cast(),
                core::mem::size_of::<TOKEN_ELEVATION>() as u32,
                &mut ret_len,
            );
            CloseHandle(token);
            if ok == 0 {
                return Err(last_err("GetTokenInformation"));
            }
            if info.TokenIsElevated == 0 {
                return Err(Error::new(
                    ErrorCode::Driver,
                    "installing WinUSB requires an elevated (Administrator) process; re-run elevated",
                ));
            }
            Ok(())
        }
    }

    /// A private temp directory to stage the generated INF in.
    fn make_temp_dir() -> Result<std::path::PathBuf> {
        let dir = std::env::temp_dir().join(format!("numanager-winusb-{}", std::process::id()));
        std::fs::create_dir_all(&dir)
            .map_err(|e| Error::new(ErrorCode::Driver, format!("cannot create temp dir: {e}")))?;
        Ok(dir)
    }

    /// Write a device-specific INF that binds the inbox `WinUSB.sys` via
    /// `Include = winusb.inf, Needs = WINUSB.NT`. No coinstaller (unnecessary on
    /// Windows 10+). Declares `CatalogFile` so the signed catalog produced by
    /// [`crate::signing`] authenticates the package. Modeled on libwdi's
    /// `winusb.inf.in`, trimmed to the inbox path.
    fn write_winusb_inf(dir: &std::path::Path, hardware_id: &str) -> Result<std::path::PathBuf> {
        // ClassGuid {88BAE032-5A81-49F0-BC3D-A4FF138216D6} = USBDevice class.
        let inf = format!(
            "; Generated by numanager-winusb. Binds the inbox WinUSB driver.\r\n\
             [Version]\r\n\
             Signature = \"$Windows NT$\"\r\n\
             Class     = USBDevice\r\n\
             ClassGuid = {{88BAE032-5A81-49F0-BC3D-A4FF138216D6}}\r\n\
             Provider  = %ProviderName%\r\n\
             CatalogFile = {CAT_FILENAME}\r\n\
             DriverVer = 01/01/2024,1.0.0.0\r\n\
             \r\n\
             [Manufacturer]\r\n\
             %ProviderName% = Standard,NTamd64,NTarm64\r\n\
             \r\n\
             [Standard.NTamd64]\r\n\
             %DeviceName% = USB_Install, {hardware_id}\r\n\
             \r\n\
             [Standard.NTarm64]\r\n\
             %DeviceName% = USB_Install, {hardware_id}\r\n\
             \r\n\
             [USB_Install]\r\n\
             Include = winusb.inf\r\n\
             Needs   = WINUSB.NT\r\n\
             \r\n\
             [USB_Install.Services]\r\n\
             Include = winusb.inf\r\n\
             Needs   = WINUSB.NT.Services\r\n\
             \r\n\
             [Strings]\r\n\
             ProviderName = \"numanager\"\r\n\
             DeviceName   = \"numanager WinUSB device\"\r\n"
        );
        let path = dir.join(INF_FILENAME);
        std::fs::write(&path, inf)
            .map_err(|e| Error::new(ErrorCode::Driver, format!("cannot write INF: {e}")))?;
        Ok(path)
    }

    /// Apply the INF to every present device matching `hardware_id` via
    /// `newdev`'s `UpdateDriverForPlugAndPlayDevices` with `INSTALLFLAG_FORCE`.
    fn update_driver(hardware_id: &str, inf_path: &std::path::Path) -> Result<()> {
        let hwid_w = wide(hardware_id);
        let inf_str = inf_path
            .to_str()
            .ok_or_else(|| Error::new(ErrorCode::Driver, "INF path is not valid UTF-8"))?;
        let inf_w = wide(inf_str);
        let mut reboot: windows_sys::core::BOOL = 0;
        // SAFETY: FFI. Both wide strings are NUL-terminated and live across the
        // call; `reboot` is a valid out-param.
        let ok = unsafe {
            UpdateDriverForPlugAndPlayDevicesW(
                core::ptr::null_mut(),
                hwid_w.as_ptr(),
                inf_w.as_ptr(),
                INSTALLFLAG_FORCE,
                &mut reboot,
            )
        };
        if ok == 0 {
            let code = unsafe { GetLastError() };
            return Err(Error::new(
                ErrorCode::Driver,
                format!(
                    "UpdateDriverForPlugAndPlayDevices failed (GetLastError=0x{code:08x}) for \
                     '{hardware_id}': the device may not be present, or an unsigned package needs \
                     an interactive Administrator to accept the publisher warning"
                ),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn already_winusb_is_idempotent_without_calling_approve() {
        // Can't fabricate a real device node here, but we can assert the
        // PortState predicates the approval logic relies on.
        assert!(PortState::WinUsb.is_winusb());
        assert!(!PortState::WinUsb.would_displace());
        assert!(!PortState::Free.is_winusb());
        assert!(PortState::Composite.would_displace());
        assert!(PortState::TakenBy("vendor.sys".into()).would_displace());
    }

    #[test]
    fn displacing_states_warn_in_the_prompt_callers_show() {
        let function = UsbFunction::new(0x1234, 0x5678);
        for state in [
            PortState::Composite,
            PortState::TakenBy("vendor.sys".into()),
        ] {
            assert!(
                InstallApproval::new(&function, &state)
                    .prompt
                    .starts_with("WARNING:"),
                "{state:?} must warn that a working driver is replaced"
            );
        }
        assert!(!InstallApproval::new(&function, &PortState::Free)
            .prompt
            .starts_with("WARNING:"));
    }

    #[test]
    fn hardware_ids_address_the_device_or_one_function() {
        let device = UsbFunction::new(0x5354, 0x009a);
        assert_eq!(device.hardware_id(), r"USB\VID_5354&PID_009A");
        assert_eq!(
            device.interface(10).hardware_id(),
            r"USB\VID_5354&PID_009A&MI_0A"
        );

        // The device query must not match a composite child, and an interface
        // query must match whether or not Windows qualifies the id with &REV_.
        assert!(device.matches_hardware_id(r"USB\VID_5354&PID_009A&REV_0100"));
        assert!(!device.matches_hardware_id(r"USB\VID_5354&PID_009A&MI_00"));
        assert!(device
            .interface(0)
            .matches_hardware_id(r"USB\VID_5354&PID_009A&REV_0100&MI_00"));
        assert!(!device
            .interface(0)
            .matches_hardware_id(r"USB\VID_5354&PID_009A&MI_01"));
    }

    #[cfg(not(windows))]
    #[test]
    fn unsupported_off_windows() {
        assert!(port_state(&UsbFunction::new(0x5354, 0x009a)).is_err());
    }
}
