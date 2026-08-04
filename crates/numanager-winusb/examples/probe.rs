//! Print the WinUSB port state for a USB device, and what installing WinUSB
//! would tell the user. Reports only — it never installs.
//!
//! Usage:
//!   cargo run -p numanager-winusb --example probe -- 5354:009a
//!   cargo run -p numanager-winusb --example probe -- 5354:009a:0     # interface 0
//!   cargo run -p numanager-winusb --example probe -- "USB\VID_5354&PID_009A"

use numanager_winusb::{port_state, port_state_by_hardware_id, PortState, UsbFunction};

fn main() {
    let target = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "5354:009a".into());

    let (label, state) = match parse_function(&target) {
        Some(function) => (function.to_string(), port_state(&function)),
        None => (target.clone(), port_state_by_hardware_id(&target)),
    };

    match state {
        Ok(state) => {
            println!("{label} -> {state:?}");
            println!("  owner          = {}", state.owner());
            println!("  is_winusb      = {}", state.is_winusb());
            println!("  would_displace = {}", state.would_displace());
            if let Some(function) = parse_function(&target) {
                println!("  diagnosis      = {}", state.diagnosis(&function));
                if let PortState::Composite = state {
                    println!(
                        "  next           = re-run as {target}:<interface> to ask about one function"
                    );
                }
            }
        }
        Err(error) => println!("{label} -> error: {error}"),
    }
}

/// Parse `vid:pid` or `vid:pid:interface`, hex without prefixes.
fn parse_function(arg: &str) -> Option<UsbFunction> {
    let mut fields = arg.split(':');
    let vendor_id = u16::from_str_radix(fields.next()?, 16).ok()?;
    let product_id = u16::from_str_radix(fields.next()?, 16).ok()?;
    let function = UsbFunction::new(vendor_id, product_id);
    match fields.next() {
        Some(interface) => Some(function.interface(interface.parse().ok()?)),
        None => Some(function),
    }
}
