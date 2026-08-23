//! Server configuration, loaded from a TOML file.

use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

/// Configuration for the dnsless server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// The hostname (e.g. `"myserver.home"`) that clients should associate
    /// with this server's IP address.
    pub hostname: String,

    /// The TCP port the server listens on for client connections.
    /// Defaults to `5353`.
    #[serde(default = "default_port")]
    pub port: u16,

    /// The name of the network interface whose IP address is monitored,
    /// e.g. `"eth0"` or `"enp3s0"`.  On Windows use the adapter description
    /// (e.g. `"Ethernet"`).  Leave blank to auto-detect the first non-loopback
    /// IPv4 interface.
    #[serde(default)]
    pub interface: String,

    /// How often (in seconds) the server polls the interface for IP changes.
    /// Defaults to `30`.
    #[serde(default = "default_poll_interval")]
    pub poll_interval_secs: u64,
}

fn default_port() -> u16 {
    5353
}

fn default_poll_interval() -> u64 {
    30
}

impl ServerConfig {
    /// Load configuration from a TOML file.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, String> {
        let content =
            fs::read_to_string(path).map_err(|e| format!("Cannot read config file: {e}"))?;
        toml::from_str(&content).map_err(|e| format!("Invalid config file: {e}"))
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            hostname: "server.home".into(),
            port: default_port(),
            interface: String::new(),
            poll_interval_secs: default_poll_interval(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let cfg = ServerConfig::default();
        assert_eq!(cfg.port, 5353);
        assert_eq!(cfg.poll_interval_secs, 30);
    }

    #[test]
    fn parse_toml() {
        let toml = r#"
hostname = "nas.home"
port = 9000
interface = "eth0"
poll_interval_secs = 60
"#;
        let cfg: ServerConfig = toml::from_str(toml).unwrap();
        assert_eq!(cfg.hostname, "nas.home");
        assert_eq!(cfg.port, 9000);
        assert_eq!(cfg.interface, "eth0");
        assert_eq!(cfg.poll_interval_secs, 60);
    }
}
