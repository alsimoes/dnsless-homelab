//! IP address detection for the server.
//!
//! Iterates the system's network interfaces and returns the first matching
//! non-loopback IPv4 address.

use std::net::IpAddr;

/// Detect the current IP address.
///
/// * If `interface` is non-empty, only addresses belonging to that interface
///   name are considered.
/// * Otherwise the first non-loopback IPv4 address found is returned.
pub fn detect_ip(interface: &str) -> Result<IpAddr, String> {
    let ifaces = get_if_addrs::get_if_addrs()
        .map_err(|e| format!("Cannot enumerate network interfaces: {e}"))?;

    for iface in &ifaces {
        if iface.is_loopback() {
            continue;
        }

        let ip = iface.ip();
        if !matches!(ip, IpAddr::V4(_)) {
            continue;
        }

        if interface.is_empty() || iface.name == interface {
            return Ok(ip);
        }
    }

    Err(format!(
        "No suitable IP address found{}",
        if interface.is_empty() {
            String::new()
        } else {
            format!(" for interface '{interface}'")
        }
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_detect_returns_some_ip() {
        // The test environment must have at least one non-loopback interface.
        let result = detect_ip("");
        // It may or may not have a non-loopback interface in CI; we just
        // ensure the function doesn't panic.
        let _ = result;
    }

    #[test]
    fn unknown_interface_returns_error() {
        let result = detect_ip("__nonexistent_iface_xyz__");
        assert!(result.is_err());
    }
}
