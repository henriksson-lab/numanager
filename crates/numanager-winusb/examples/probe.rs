//! Print the WinUSB port state for a device hardware-id prefix.
//!
//! Usage: cargo run -p numanager-winusb --example probe -- "USB\VID_5354&PID_009A"

fn main() {
    let hwid = std::env::args()
        .nth(1)
        .unwrap_or_else(|| r"USB\VID_5354&PID_009A".to_string());
    match numanager_winusb::port_state(&hwid) {
        Ok(state) => {
            println!("{hwid} -> {state:?}");
            println!("  is_winusb      = {}", state.is_winusb());
            println!("  would_displace = {}", state.would_displace());
        }
        Err(e) => println!("{hwid} -> error: {e}"),
    }
}
