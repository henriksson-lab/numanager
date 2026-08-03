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
//! Everything here is Windows-only. On other platforms the entry points return
//! [`ErrorCode::Unsupported`] so callers can be written once and gated by result.

use numanager_core::{Error, ErrorCode, Result};

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
    /// Another driver owns the node (its service name, e.g. `usbccgp` or a
    /// vendor `.sys`). Installing WinUSB would replace a working driver, so
    /// callers should warn before proceeding.
    TakenBy(String),
}

impl PortState {
    /// Whether WinUSB is already bound.
    pub fn is_winusb(&self) -> bool {
        matches!(self, PortState::WinUsb)
    }

    /// Whether installing WinUSB would displace an existing, working driver.
    /// Callers should surface a warning to the user when this is true.
    pub fn would_displace(&self) -> bool {
        matches!(self, PortState::TakenBy(_))
    }
}

/// Report which driver owns the first present USB device whose hardware id
/// contains `hardware_id_prefix` (case-insensitive), e.g.
/// `"USB\\VID_5354&PID_009A"`.
///
/// Matching on a prefix rather than the full id lets a caller ignore the
/// trailing `&REV_xxxx`. For a composite device WinUSB binds per interface
/// (`…&MI_00`); pass that fuller id to target one function.
///
/// Errors if no present device matches, or on a SetupAPI failure.
#[cfg(windows)]
pub fn port_state(hardware_id_prefix: &str) -> Result<PortState> {
    win::port_state(hardware_id_prefix)
}

/// Non-Windows stub: WinUSB provisioning does not apply.
#[cfg(not(windows))]
pub fn port_state(_hardware_id_prefix: &str) -> Result<PortState> {
    Err(Error::new(
        ErrorCode::Unsupported,
        "WinUSB provisioning is only available on Windows",
    ))
}

/// Ensure WinUSB is bound to the device identified by `hardware_id_prefix`.
///
/// * If WinUSB is already bound, returns `Ok(())` (idempotent).
/// * Otherwise `approve` is called with the current [`PortState`] — `Free` for a
///   clean install, `TakenBy(_)` when an existing driver would be displaced — so
///   the caller can prompt (and phrase the warning differently for the two). If
///   it returns `false`, this returns [`ErrorCode::Cancelled`] and touches
///   nothing.
/// * On approval, the install runs. It requires elevation; on a non-elevated
///   process the backend surfaces that as an error rather than silently failing.
pub fn ensure_winusb(hardware_id_prefix: &str, approve: &dyn Fn(&PortState) -> bool) -> Result<()> {
    let state = port_state(hardware_id_prefix)?;
    if state.is_winusb() {
        return Ok(());
    }
    if !approve(&state) {
        return Err(Error::new(
            ErrorCode::Cancelled,
            "WinUSB installation was not approved",
        ));
    }
    install_winusb(hardware_id_prefix, &state)
}

/// Perform the install: require elevation, write a WinUSB INF, sign the package,
/// and apply it. See the module docs for why the install is native rather than
/// libwdi; signing is a faithful port of libwdi's `pki.c` (see [`signing`]).
#[cfg(windows)]
fn install_winusb(hardware_id_prefix: &str, _state: &PortState) -> Result<()> {
    win::install(hardware_id_prefix)
}

#[cfg(not(windows))]
fn install_winusb(_hardware_id_prefix: &str, _state: &PortState) -> Result<()> {
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
    use windows_sys::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    /// `SetupDiGetClassDevsW` returns `INVALID_HANDLE_VALUE` (-1 as a handle) on
    /// failure; `HDEVINFO` is an `isize`, so compare against that.
    const INVALID_DEVINFO: HDEVINFO = -1;

    pub(super) fn port_state(hardware_id_prefix: &str) -> Result<PortState> {
        let target = hardware_id_prefix.to_ascii_uppercase();
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
                let matches = decode_multi_sz(&hwids)
                    .iter()
                    .any(|h| h.to_ascii_uppercase().contains(&target));
                if !matches {
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
                } else {
                    PortState::TakenBy(service)
                });
                break;
            }

            SetupDiDestroyDeviceInfoList(set);
            found.ok_or_else(|| {
                Error::new(
                    ErrorCode::Transport,
                    format!("no present USB device matches hardware id '{hardware_id_prefix}'"),
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
        assert!(PortState::TakenBy("usbccgp".into()).would_displace());
    }

    #[cfg(not(windows))]
    #[test]
    fn unsupported_off_windows() {
        assert!(port_state("USB\\VID_5354&PID_009A").is_err());
    }
}
