use numanager_core::runtime::{DiscoveryRegistry, DriverCandidate, DriverDiscovery};
use numanager_core::{DriverId, Result};

pub struct UsbVidPidDiscovery {
    registry: DiscoveryRegistry,
}

impl UsbVidPidDiscovery {
    pub fn new() -> Self {
        let mut registry = DiscoveryRegistry::new();
        register_builtin_usb_vid_pid_discovery(&mut registry);
        Self { registry }
    }

    pub fn with_next_id(next_id: DriverId) -> Self {
        let mut registry = DiscoveryRegistry::with_next_id(next_id);
        register_builtin_usb_vid_pid_discovery(&mut registry);
        Self { registry }
    }

    pub fn is_empty(&self) -> bool {
        self.registry.is_empty()
    }
}

impl Default for UsbVidPidDiscovery {
    fn default() -> Self {
        Self::new()
    }
}

/// A USB vendor id that a builtin driver claims, and the driver claiming it.
///
/// A host that opens these devices from userspace needs access to them — on
/// Linux, a udev rule per vendor id. Downstream applications should generate
/// their rules from [`builtin_usb_vendor_claims`] rather than keep a vendor
/// list of their own, which would silently go stale as drivers are added.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct UsbVendorClaim {
    pub driver: &'static str,
    pub vendor_id: u16,
}

/// Declares the builtin USB VID/PID drivers **once**, and derives both the
/// discovery registration and the vendor-id claims from that single list — so
/// the access rules a host installs cannot drift from the drivers that probe.
macro_rules! builtin_usb_drivers {
    ($($driver:literal => $module:ident :: $discovery:ident),* $(,)?) => {
        /// Register every builtin USB VID/PID discovery.
        pub fn register_builtin_usb_vid_pid_discovery(registry: &mut DiscoveryRegistry) {
            #[cfg(feature = "os-usb")]
            {
                $( registry.register_factory(crate::$module::$discovery::os_usb); )*
            }

            #[cfg(not(feature = "os-usb"))]
            {
                let _ = registry;
            }
        }

        /// Every USB vendor id the builtin drivers claim, sorted and deduped.
        ///
        /// Available without the `os-usb` feature: a packaging step that only
        /// generates access rules should not have to build the USB transport.
        pub fn builtin_usb_vendor_claims() -> Vec<UsbVendorClaim> {
            let mut claims = Vec::new();
            $(
                for vendor_id in crate::$module::usb_vendor_ids() {
                    claims.push(UsbVendorClaim { driver: $driver, vendor_id });
                }
            )*
            claims.sort_unstable();
            claims.dedup();
            claims
        }

        /// The drivers behind [`builtin_usb_vendor_claims`], in declaration order.
        pub fn builtin_usb_driver_names() -> Vec<&'static str> {
            vec![$($driver),*]
        }
    };
}

builtin_usb_drivers! {
    "andor-camera" => andor_camera::AndorCameraDiscovery,
    "lumenera" => lumenera::LumeneraDiscovery,
    "mcl" => mcl::MclDiscovery,
    "toupcam" => toupcam::ToupcamDiscovery,
    "photometrics-pvcam" => photometrics_pvcam::PvcamDiscovery,
    "velleman" => velleman::VellemanDiscovery,
}

impl DriverDiscovery for UsbVidPidDiscovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        self.registry.detect_all()
    }
}

/// Host-access diagnosis to append to a failed USB interface claim, already
/// punctuated for that position (`"; …"`), or empty when the host has nothing
/// to add.
///
/// A userspace USB driver can only claim an interface the host has granted it:
/// a udev rule on Linux, a WinUSB binding on Windows. Windows is the case that
/// needs explaining at the point of failure — `nusb` reports only
/// `Device driver is "…", not WinUSB`, which does not tell the user what to do
/// — so on Windows this asks the `winusb_access` module what owns the node
/// (that module is not compiled on other hosts, hence no link). See
/// [`builtin_usb_vendor_claims`] for the Linux side.
///
/// This reports; it never changes a driver binding. Empty off Windows.
pub fn usb_claim_hint(vendor_id: u16, product_id: u16, interface: u8) -> String {
    #[cfg(any(windows, feature = "winusb"))]
    {
        let function =
            crate::winusb_access::UsbFunction::new(vendor_id, product_id).interface(interface);
        match crate::winusb_access::claim_failure_hint(function) {
            Some(hint) => format!("; {hint}"),
            None => String::new(),
        }
    }

    #[cfg(not(any(windows, feature = "winusb")))]
    {
        let _ = (vendor_id, product_id, interface);
        String::new()
    }
}
