//! Shared protocol types for dnsless-homelab.
//!
//! The server sends [`Message`] values serialized as newline-delimited JSON
//! over a persistent TCP connection to every connected client.

use std::net::SocketAddr;

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

/// Observability event emitted by the server's background loop.
///
/// This is an admin/observability channel, **not** part of the [`Message`]
/// wire protocol. It lives in `dnsless_common` so the native UI, the WASM
/// web UI, and the server can all share the exact same type (the WASM target
/// cannot depend on `dnsless_server`, which pulls in native-only crates).
///
/// Clients are identified only by [`SocketAddr`]; a "list of clients by
/// hostname" does not exist in the current protocol (lacuna nº 1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerEvent {
    /// The TCP listener bound successfully and is accepting connections.
    Listening { bind_addr: SocketAddr },
    /// A client connected (identified only by socket address).
    ClientConnected { peer: SocketAddr },
    /// A client disconnected or was removed during a broadcast.
    ClientDisconnected { peer: SocketAddr },
    /// The monitored interface's IP changed (or was detected the first time).
    IpChanged {
        hostname: String,
        ip: String,
        is_initial: bool,
    },
    /// A heartbeat was broadcast to all clients.
    HeartbeatSent,
    /// The poll loop ticked and the IP was unchanged.
    PollUnchanged { ip: String },
    /// A non-fatal operational error (accept failure, IP detection failure, ...).
    Error { message: String },
}

/// Observability event emitted by the client's background loop.
///
/// Like [`ServerEvent`], this is an admin channel shared via `dnsless_common`
/// so the native UI and the WASM web UI use the same type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientEvent {
    /// Attempting to connect to the configured server.
    Connecting { server_addr: String },
    /// TCP connection to the server succeeded.
    Connected { server_addr: String },
    /// Connection lost (read error or EOF); will reconnect after delay.
    ConnectionLost { server_addr: String },
    /// Connection attempt failed; will retry after delay.
    ConnectionFailed { server_addr: String, error: String },
    /// An IP-update message was received from the server.
    IpUpdateReceived { hostname: String, ip: String },
    /// The hosts file was successfully updated.
    HostsFileUpdated { hostname: String, ip: String },
    /// The hosts file update FAILED (typically: permission denied).
    /// The UI must surface this prominently; never swallow it.
    HostsFileError {
        hostname: String,
        ip: String,
        error: String,
    },
    /// A heartbeat was received (connection still alive).
    HeartbeatReceived,
    /// A message from the server could not be parsed.
    ParseError { raw: String, error: String },
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

    #[test]
    fn round_trip_server_event() {
        let ev = ServerEvent::ClientConnected {
            peer: "192.168.1.10:12345".parse().unwrap(),
        };
        let json = serde_json::to_string(&ev).unwrap();
        assert!(json.contains("client_connected"));
        let decoded: ServerEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(ev, decoded);
    }

    #[test]
    fn round_trip_client_event() {
        let ev = ClientEvent::HostsFileError {
            hostname: "nas.home".into(),
            ip: "10.0.0.5".into(),
            error: "Permission denied".into(),
        };
        let json = serde_json::to_string(&ev).unwrap();
        assert!(json.contains("hosts_file_error"));
        let decoded: ClientEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(ev, decoded);
    }
}
