//! A [`Transport`] over a Spark reader's USB interface.
//!
//! This is an adapter, not a protocol. It moves bytes between the device and the rest of the
//! driver, which owns the framing ([`super::tdcl`]), the command vocabulary
//! ([`super::commands`]) and the decoder ([`super::data`]).
//!
//! # Channels
//!
//! The reader presents three logical channels on one interface, mapped onto endpoints in
//! ascending address order: commands go out on BULK-OUT #0, their replies arrive on
//! INTERRUPT-IN #0, and measurement data arrives on BULK-IN #0. A second INTERRUPT-IN
//! carries firmware log output and is not read here.
//!
//! Both inbound endpoints are drained into one queue. Nothing is lost by interleaving them:
//! a TDCL frame's type byte says which channel it belongs to, and the session matches
//! replies to requests by sequence number rather than by arrival order.
//!
//! # What is not established
//!
//! The reader's **VID/PID is unknown** — it is not in the recovered evidence, and no capture
//! exists. There is no default: [`UsbConfig`] takes it from configuration, and opening
//! without one fails with a message saying so rather than probing for something plausible.
//! The endpoint roles above come from the vendor stack's channel model, and want confirming
//! against a real descriptor. See `docs/reverse/spark-cyto.md`.

use numanager_core::{Error, ErrorCode};
#[cfg(not(feature = "os-usb"))]
use numanager_core::{Result, Transport};
#[cfg(not(feature = "os-usb"))]
use std::collections::VecDeque;
use std::time::Duration;

/// Which reader to open.
#[derive(Debug, Clone)]
pub struct UsbConfig {
    pub vendor_id: u16,
    pub product_id: u16,
    /// Serial-number filter, for a bench with more than one reader.
    pub serial: Option<String>,
    /// How much of a reply to ask for in one read. Frames are chunked at 65530 bytes.
    pub read_size: usize,
    /// How long to wait for a reply before reporting that nothing arrived.
    pub timeout: Duration,
}

impl Default for UsbConfig {
    fn default() -> Self {
        UsbConfig {
            vendor_id: 0,
            product_id: 0,
            serial: None,
            read_size: 65_536,
            timeout: Duration::from_millis(200),
        }
    }
}

impl UsbConfig {
    pub fn new(vendor_id: u16, product_id: u16) -> Self {
        Self {
            vendor_id,
            product_id,
            ..Self::default()
        }
    }

    pub fn with_serial(mut self, serial: impl Into<String>) -> Self {
        self.serial = Some(serial.into());
        self
    }
}

/// The endpoint addresses one interface offers, sorted by address within each role.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Endpoints {
    pub interrupt_in: Vec<u8>,
    pub bulk_in: Vec<u8>,
    pub bulk_out: Vec<u8>,
}

/// The three channel endpoints: `(command_in, command_out, data_in)`.
///
/// `None` when any role is missing, which means this interface is not the channel interface.
pub fn pick_endpoints(endpoints: &Endpoints) -> Option<(u8, u8, u8)> {
    Some((
        *endpoints.interrupt_in.first()?,
        *endpoints.bulk_out.first()?,
        *endpoints.bulk_in.first()?,
    ))
}

#[cfg_attr(not(feature = "os-usb"), allow(dead_code))]
fn usb_error(message: impl Into<String>) -> Error {
    Error::new(ErrorCode::Transport, message.into())
}

fn no_ids() -> Error {
    Error::new(
        ErrorCode::InvalidProperty,
        "the Spark reader's USB vendor/product id is not configured, and this driver has \
         none to fall back on: the id is not in the recovered evidence, so it has to come \
         from an `lsusb -v` on the instrument (see docs/reverse/spark-cyto.md)",
    )
}

#[cfg(feature = "os-usb")]
pub use live::UsbTransport;

#[cfg(feature = "os-usb")]
mod live {
    use super::{no_ids, pick_endpoints, usb_error, Endpoints, UsbConfig};
    use futures_lite::future::block_on;
    use numanager_core::{Result, Transport};
    use nusb::transfer::RequestBuffer;
    use std::collections::VecDeque;

    pub struct UsbTransport {
        interface: nusb::Interface,
        command_in: u8,
        command_out: u8,
        data_in: u8,
        read_size: usize,
        inbound: VecDeque<Vec<u8>>,
    }

    impl UsbTransport {
        /// Open the first reader matching `config` and claim its channel interface.
        pub fn open(config: &UsbConfig) -> Result<Self> {
            if config.vendor_id == 0 || config.product_id == 0 {
                return Err(no_ids());
            }
            let candidate = nusb::list_devices()
                .map_err(|error| usb_error(format!("USB device listing failed: {error}")))?
                .find(|device| {
                    device.vendor_id() == config.vendor_id
                        && device.product_id() == config.product_id
                        && match &config.serial {
                            Some(serial) => device.serial_number() == Some(serial.as_str()),
                            None => true,
                        }
                })
                .ok_or_else(|| {
                    usb_error(format!(
                        "no USB device matches the configured Spark reader {:04x}:{:04x}",
                        config.vendor_id, config.product_id
                    ))
                })?;

            let device = candidate.open().map_err(|error| {
                usb_error(format!(
                    "opening Spark reader {:04x}:{:04x} failed: {error}{}",
                    config.vendor_id,
                    config.product_id,
                    crate::usb_discovery::usb_claim_hint(config.vendor_id, config.product_id, 0)
                ))
            })?;

            let configuration = device.active_configuration().map_err(|error| {
                usb_error(format!("reading the active configuration failed: {error}"))
            })?;

            let mut interfaces: Vec<(u8, Endpoints)> = Vec::new();
            for interface in configuration.interface_alt_settings() {
                if interface.alternate_setting() != 0 {
                    continue;
                }
                let mut endpoints = Endpoints::default();
                for endpoint in interface.endpoints() {
                    match (endpoint.transfer_type(), endpoint.direction()) {
                        (nusb::transfer::EndpointType::Bulk, nusb::transfer::Direction::In) => {
                            endpoints.bulk_in.push(endpoint.address())
                        }
                        (nusb::transfer::EndpointType::Bulk, nusb::transfer::Direction::Out) => {
                            endpoints.bulk_out.push(endpoint.address())
                        }
                        (
                            nusb::transfer::EndpointType::Interrupt,
                            nusb::transfer::Direction::In,
                        ) => endpoints.interrupt_in.push(endpoint.address()),
                        _ => {}
                    }
                }
                endpoints.interrupt_in.sort_unstable();
                endpoints.bulk_in.sort_unstable();
                endpoints.bulk_out.sort_unstable();
                interfaces.push((interface.interface_number(), endpoints));
            }

            let (number, endpoints) = interfaces
                .iter()
                .find(|(_, endpoints)| pick_endpoints(endpoints).is_some())
                .ok_or_else(|| {
                    usb_error(
                        "no interface on this device carries the channel endpoints a reader \
                         needs (INTERRUPT-IN, BULK-OUT and BULK-IN); if this is the right \
                         device, its descriptor disagrees with the recorded channel model",
                    )
                })?;
            let (command_in, command_out, data_in) =
                pick_endpoints(endpoints).expect("checked just above");

            let interface = device
                .detach_and_claim_interface(*number)
                .map_err(|error| {
                    usb_error(format!(
                        "claiming Spark reader interface {number} failed: {error}{}",
                        crate::usb_discovery::usb_claim_hint(
                            config.vendor_id,
                            config.product_id,
                            *number
                        )
                    ))
                })?;

            Ok(Self {
                interface,
                command_in,
                command_out,
                data_in,
                read_size: config.read_size.max(1),
                inbound: VecDeque::new(),
            })
        }

        /// The endpoints in use: `(command_in, command_out, data_in)`.
        pub fn endpoints(&self) -> (u8, u8, u8) {
            (self.command_in, self.command_out, self.data_in)
        }

        /// Take whatever either inbound endpoint has ready.
        ///
        /// A read that returns nothing is the normal idle case, not an error: the session
        /// polls, and an instrument that is still working has not answered yet.
        fn pump(&mut self) {
            let replies = block_on(
                self.interface
                    .interrupt_in(self.command_in, RequestBuffer::new(self.read_size)),
            );
            if let Ok(bytes) = replies.into_result() {
                if !bytes.is_empty() {
                    self.inbound.push_back(bytes);
                }
            }
            let data = block_on(
                self.interface
                    .bulk_in(self.data_in, RequestBuffer::new(self.read_size)),
            );
            if let Ok(bytes) = data.into_result() {
                if !bytes.is_empty() {
                    self.inbound.push_back(bytes);
                }
            }
        }
    }

    impl Transport for UsbTransport {
        fn send(&mut self, bytes: &[u8]) -> Result<()> {
            block_on(self.interface.bulk_out(self.command_out, bytes.to_vec()))
                .into_result()
                .map(|_| ())
                .map_err(|error| usb_error(format!("Spark command write failed: {error}")))
        }

        fn poll_recv(&mut self) -> Result<Option<Vec<u8>>> {
            if self.inbound.is_empty() {
                self.pump();
            }
            Ok(self.inbound.pop_front())
        }
    }
}

/// Without the `os-usb` feature there is no OS transport to open.
#[cfg(not(feature = "os-usb"))]
pub struct UsbTransport {
    inbound: VecDeque<Vec<u8>>,
}

#[cfg(not(feature = "os-usb"))]
impl UsbTransport {
    pub fn open(config: &UsbConfig) -> Result<Self> {
        if config.vendor_id == 0 || config.product_id == 0 {
            return Err(no_ids());
        }
        Err(Error::new(
            ErrorCode::Unsupported,
            "reaching a Spark reader over USB needs the `os-usb` feature",
        ))
    }
}

#[cfg(not(feature = "os-usb"))]
impl Transport for UsbTransport {
    fn send(&mut self, _bytes: &[u8]) -> Result<()> {
        Err(Error::new(
            ErrorCode::Unsupported,
            "reaching a Spark reader over USB needs the `os-usb` feature",
        ))
    }

    fn poll_recv(&mut self) -> Result<Option<Vec<u8>>> {
        Ok(self.inbound.pop_front())
    }
}
