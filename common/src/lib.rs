//! Shared protocol types for dnsless-homelab.
//!
//! The server sends [`Message`] values serialized as newline-delimited JSON
//! over a persistent TCP connection to every connected client.

use serde::{Deserialize, Serialize};

/// A protocol message sent from the server to clients.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Message {
    /// The server's IP address on the named network interface has changed.
    IpUpdate(IpUpdate),
    /// Heartbeat / keep-alive sent periodically so the client can detect
    /// a dropped connection quickly.
    Heartbeat,
}

/// Carries the new IP address together with the hostname that clients should
/// map to that address in their hosts file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpUpdate {
    /// The human-readable hostname, e.g. `"myserver.home"`.
    pub hostname: String,
    /// The new IP address as a string, e.g. `"192.168.1.42"`.
    pub ip: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ip_update() {
        let msg = Message::IpUpdate(IpUpdate {
            hostname: "box.home".into(),
            ip: "10.0.0.1".into(),
        });
        let json = serde_json::to_string(&msg).unwrap();
        let decoded: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn round_trip_heartbeat() {
        let msg = Message::Heartbeat;
        let json = serde_json::to_string(&msg).unwrap();
        let decoded: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, decoded);
    }
}
