//! USB host-access setup: who owns a device node, and granting numanager access.
//!
//! A userspace USB driver can only claim an interface the host has granted it.
//! On Windows that means WinUSB must be bound to the node; while a vendor
//! driver owns it, every `claim_interface` fails and the driver's own error
//! can only guess why. This example reports the real owner and, on request,
//! binds WinUSB behind the approval gate.
//!
//! It exists because bringing up any reverse-engineered USB device runs the
//! same loop — capture traffic under the vendor driver, hand the node to
//! numanager, test, hand it back — and that loop needs a supported way to move
//! the binding rather than a one-off script per device.
//!
//! ```sh
//! cargo run -p numanager-examples -- usb_access claims
//! cargo run -p numanager-examples -- usb_access show 5354:009a
//! cargo run -p numanager-examples -- usb_access bind 5354:009a --approve
//! ```
//!
//! `bind` **displaces whatever driver currently owns the node** and needs an
//! elevated process, so it does nothing without `--approve`. It is a
//! deliberate setup action, not something a driver should do while opening a
//! device. Nothing here is reversible from numanager: to give the node back to
//! a vendor driver, reinstall that driver's package.

use numanager_core::*;
use numanager_drivers::usb_discovery::{builtin_usb_driver_names, builtin_usb_vendor_claims};
use numanager_examples::example_arg;

pub fn run() -> Result<()> {
    match example_arg(0).unwrap_or_else(|| "claims".into()).as_str() {
        "claims" => claims(),
        "show" => show(&function_arg()?),
        "bind" => bind(&function_arg()?),
        other => Err(Error::new(
            ErrorCode::InvalidCommand,
            format!("unknown usb_access mode {other}; expected claims, show, or bind"),
        )),
    }
}

/// Which USB vendor ids the builtin drivers claim. This is also what a
/// packaging step turns into Linux udev rules, so it needs no OS USB support.
fn claims() -> Result<()> {
    let claims = builtin_usb_vendor_claims();
    println!(
        "{} vendor claim(s) across {} builtin USB driver(s):",
        claims.len(),
        builtin_usb_driver_names().len()
    );
    for claim in &claims {
        println!("  {:04x}  {}", claim.vendor_id, claim.driver);
    }
    Ok(())
}

/// `VID:PID` in hex, as USB tools print it.
fn function_arg() -> Result<(u16, u16)> {
    let raw = example_arg(1).ok_or_else(|| {
        Error::new(
            ErrorCode::InvalidCommand,
            "expected a device as VID:PID in hex, e.g. 5354:009a",
        )
    })?;
    let (vendor, product) = raw.split_once(':').ok_or_else(|| {
        Error::new(
            ErrorCode::InvalidCommand,
            format!("malformed device {raw}; expected VID:PID in hex"),
        )
    })?;
    let parse = |text: &str, what: &str| {
        u16::from_str_radix(text.trim_start_matches("0x"), 16).map_err(|error| {
            Error::new(
                ErrorCode::InvalidCommand,
                format!("malformed {what} {text}: {error}"),
            )
        })
    };
    Ok((parse(vendor, "vendor id")?, parse(product, "product id")?))
}

#[cfg(any(windows, feature = "winusb"))]
fn show(&(vendor_id, product_id): &(u16, u16)) -> Result<()> {
    use numanager_drivers::winusb_access::{access_state, UsbFunction};

    let function = UsbFunction::new(vendor_id, product_id);
    let state = access_state(function)?;
    println!("device:       {vendor_id:04x}:{product_id:04x}");
    println!("owner:        {}", state.owner());
    println!("winusb bound: {}", state.is_winusb());
    println!("would displace a driver: {}", state.would_displace());
    if !state.is_winusb() {
        println!("diagnosis:    {}", state.diagnosis(&function));
    }
    Ok(())
}

#[cfg(any(windows, feature = "winusb"))]
fn bind(&(vendor_id, product_id): &(u16, u16)) -> Result<()> {
    use numanager_drivers::winusb_access::{ensure_access, UsbFunction};

    // The gate is the whole point: binding WinUSB takes the node away from
    // whatever owns it, so approval is an explicit argument rather than a
    // default. The callback still prints what it approves.
    let approved = std::env::args().any(|arg| arg == "--approve");
    let function = UsbFunction::new(vendor_id, product_id);

    ensure_access(function, &|approval| {
        println!("{}", approval.prompt);
        println!("hardware id: {}", approval.hardware_id);
        println!("current owner: {}", approval.state.owner());
        if !approved {
            println!("not approved: re-run with --approve to bind WinUSB");
        }
        approved
    })?;

    println!("WinUSB is bound to {vendor_id:04x}:{product_id:04x}");
    Ok(())
}

#[cfg(not(any(windows, feature = "winusb")))]
fn show(_device: &(u16, u16)) -> Result<()> {
    Err(unsupported())
}

#[cfg(not(any(windows, feature = "winusb")))]
fn bind(_device: &(u16, u16)) -> Result<()> {
    Err(unsupported())
}

#[cfg(not(any(windows, feature = "winusb")))]
fn unsupported() -> Error {
    Error::new(
        ErrorCode::Unsupported,
        "USB host-access inspection is a Windows facility; build with \
         numanager-drivers/winusb to compile it elsewhere",
    )
}
