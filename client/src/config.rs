//! Client configuration, loaded from a TOML file.

use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

/// Configuration for the dnsless client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientConfig {
    /// Hostname or IP address of the dnsless server to connect to.
    pub server_host: String,

    /// TCP port of the dnsless server. Defaults to `5353`.
    #[serde(default = "default_port")]
    pub server_port: u16,

    /// Path to the hosts file that should be updated.
    /// Defaults to `/etc/hosts` on Linux and
    /// `C:\Windows\System32\drivers\etc\hosts` on Windows.
    #[serde(default = "default_hosts_file")]
    pub hosts_file: String,

    /// How many seconds to wait before attempting to reconnect after a
    /// connection failure. Defaults to `10`.
    #[serde(default = "default_reconnect_delay")]
    pub reconnect_delay_secs: u64,
}

fn default_port() -> u16 {
    5353
}

fn default_hosts_file() -> String {
    if cfg!(windows) {
        r"C:\Windows\System32\drivers\etc\hosts".into()
    } else {
        "/etc/hosts".into()
    }
}

fn default_reconnect_delay() -> u64 {
    10
}

impl ClientConfig {
    /// Load configuration from a TOML file.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, String> {
        let content =
            fs::read_to_string(path).map_err(|e| format!("Cannot read config file: {e}"))?;
        toml::from_str(&content).map_err(|e| format!("Invalid config file: {e}"))
    }
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            server_host: "127.0.0.1".into(),
            server_port: default_port(),
            hosts_file: default_hosts_file(),
            reconnect_delay_secs: default_reconnect_delay(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let cfg = ClientConfig::default();
        assert_eq!(cfg.server_port, 5353);
        assert_eq!(cfg.reconnect_delay_secs, 10);
    }

    #[test]
    fn parse_toml() {
        let toml = r#"
server_host = "192.168.1.100"
server_port = 9000
hosts_file = "/tmp/hosts"
reconnect_delay_secs = 5
"#;
        let cfg: ClientConfig = toml::from_str(toml).unwrap();
        assert_eq!(cfg.server_host, "192.168.1.100");
        assert_eq!(cfg.server_port, 9000);
        assert_eq!(cfg.hosts_file, "/tmp/hosts");
        assert_eq!(cfg.reconnect_delay_secs, 5);
    }
}
