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
