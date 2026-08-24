//! Pure transformations from network events into UI log entries.
//!
//! No I/O, no egui — exhaustively match every variant so the compiler
//! forces an update whenever the event enums grow.

use dnsless_client::ClientEvent;
use dnsless_server::ServerEvent;

use crate::state::{LogEntry, LogKind};

/// Build a local-timestamped [`LogEntry`] from a server-side event.
pub fn server_event_to_log_entry(ev: &ServerEvent) -> LogEntry {
    let (text, kind) = match ev {
        ServerEvent::Listening { bind_addr } => {
            (format!("Listening on {bind_addr}"), LogKind::Info)
        }
        ServerEvent::ClientConnected { peer } => {
            (format!("Client connected: {peer}"), LogKind::Info)
        }
        ServerEvent::ClientDisconnected { peer } => {
            (format!("Client disconnected: {peer}"), LogKind::Warn)
        }
        ServerEvent::IpChanged {
            hostname,
            ip,
            is_initial,
        } => {
            let tag = if *is_initial {
                "Initial IP"
            } else {
                "IP changed"
            };
            (format!("{tag}: {hostname} -> {ip}"), LogKind::Info)
        }
        ServerEvent::HeartbeatSent => ("Heartbeat sent".to_string(), LogKind::Info),
        ServerEvent::PollUnchanged { ip } => (format!("IP unchanged: {ip}"), LogKind::Info),
        ServerEvent::Error { message } => (format!("Error: {message}"), LogKind::Error),
    };
    LogEntry {
        timestamp: now_str(),
        text,
        kind,
    }
}

/// Build a local-timestamped [`LogEntry`] from a client-side event.
pub fn client_event_to_log_entry(ev: &ClientEvent) -> LogEntry {
    let (text, kind) = match ev {
        ClientEvent::Connecting { server_addr } => {
            (format!("Connecting to {server_addr}…"), LogKind::Info)
        }
        ClientEvent::Connected { server_addr } => {
            (format!("Connected to {server_addr}"), LogKind::Info)
        }
        ClientEvent::ConnectionLost { server_addr } => (
            format!("Connection lost to {server_addr}; reconnecting"),
            LogKind::Warn,
        ),
        ClientEvent::ConnectionFailed { server_addr, error } => (
            format!("Cannot connect to {server_addr}: {error}"),
            LogKind::Error,
        ),
        ClientEvent::IpUpdateReceived { hostname, ip } => (
            format!("Received IP update: {hostname} -> {ip}"),
            LogKind::Info,
        ),
        ClientEvent::HostsFileUpdated { hostname, ip } => (
            format!("Hosts file updated: {hostname} -> {ip}"),
            LogKind::Info,
        ),
        ClientEvent::HostsFileError {
            hostname,
            ip,
            error,
        } => (
            format!("Failed to update hosts file ({hostname} -> {ip}): {error}"),
            LogKind::Error,
        ),
        ClientEvent::HeartbeatReceived => ("Heartbeat received".to_string(), LogKind::Info),
        ClientEvent::ParseError { raw, error } => (
            format!("Failed to parse message ({error}): {raw}"),
            LogKind::Warn,
        ),
    };
    LogEntry {
        timestamp: now_str(),
        text,
        kind,
    }
}

fn now_str() -> String {
    use chrono::Local;
    Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

// A tiny helper used only by tests to assert socket formatting.
#[cfg(test)]
fn peer_str(p: u16) -> String {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), p).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    fn peer(p: u16) -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), p)
    }

    fn assert_text(entry: &LogEntry, needle: &str) {
        assert!(
            entry.text.contains(needle),
            "expected log text {:?} to contain {:?}",
            entry.text,
            needle
        );
    }

    #[test]
    fn server_listening() {
        let e = server_event_to_log_entry(&ServerEvent::Listening {
            bind_addr: "0.0.0.0:5353".parse().unwrap(),
        });
        assert_text(&e, "Listening");
        assert_text(&e, "0.0.0.0:5353");
        assert_eq!(e.kind, LogKind::Info);
    }

    #[test]
    fn server_client_connected_carries_peer() {
        let e = server_event_to_log_entry(&ServerEvent::ClientConnected { peer: peer(7777) });
        assert_text(&e, "Client connected");
        assert_text(&e, &peer_str(7777));
        assert_eq!(e.kind, LogKind::Info);
    }

    #[test]
    fn server_client_disconnected_is_warn() {
        let e = server_event_to_log_entry(&ServerEvent::ClientDisconnected { peer: peer(7778) });
        assert_text(&e, "disconnected");
        assert_eq!(e.kind, LogKind::Warn);
    }

    #[test]
    fn server_ip_change_initial_vs_change() {
        let initial = server_event_to_log_entry(&ServerEvent::IpChanged {
            hostname: "nas.home".into(),
            ip: "10.0.0.5".into(),
            is_initial: true,
        });
        assert_text(&initial, "Initial IP");
        assert_text(&initial, "nas.home -> 10.0.0.5");

        let changed = server_event_to_log_entry(&ServerEvent::IpChanged {
            hostname: "nas.home".into(),
            ip: "10.0.0.6".into(),
            is_initial: false,
        });
        assert_text(&changed, "IP changed");
        assert!(!changed.text.contains("Initial"));
    }

    #[test]
    fn server_heartbeat_and_poll() {
        let h = server_event_to_log_entry(&ServerEvent::HeartbeatSent);
        assert_text(&h, "Heartbeat");
        assert_eq!(h.kind, LogKind::Info);

        let p = server_event_to_log_entry(&ServerEvent::PollUnchanged {
            ip: "1.2.3.4".into(),
        });
        assert_text(&p, "unchanged");
        assert_text(&p, "1.2.3.4");
    }

    #[test]
    fn server_error_is_error_kind() {
        let e = server_event_to_log_entry(&ServerEvent::Error {
            message: "boom".into(),
        });
        assert_text(&e, "Error");
        assert_text(&e, "boom");
        assert_eq!(e.kind, LogKind::Error);
    }

    #[test]
    fn client_connecting_and_connected() {
        let c = client_event_to_log_entry(&ClientEvent::Connecting {
            server_addr: "1.2.3.4:5353".into(),
        });
        assert_text(&c, "Connecting");
        assert_text(&c, "1.2.3.4:5353");
        assert_eq!(c.kind, LogKind::Info);

        let c = client_event_to_log_entry(&ClientEvent::Connected {
            server_addr: "1.2.3.4:5353".into(),
        });
        assert_text(&c, "Connected");
    }

    #[test]
    fn client_connection_failed_is_error() {
        let c = client_event_to_log_entry(&ClientEvent::ConnectionFailed {
            server_addr: "1.2.3.4:5353".into(),
            error: "refused".into(),
        });
        assert_text(&c, "refused");
        assert_eq!(c.kind, LogKind::Error);
    }

    #[test]
    fn client_connection_lost_is_warn() {
        let c = client_event_to_log_entry(&ClientEvent::ConnectionLost {
            server_addr: "1.2.3.4:5353".into(),
        });
        assert_eq!(c.kind, LogKind::Warn);
    }

    #[test]
    fn client_hosts_file_updated_and_error() {
        let u = client_event_to_log_entry(&ClientEvent::HostsFileUpdated {
            hostname: "nas.home".into(),
            ip: "10.0.0.5".into(),
        });
        assert_text(&u, "Hosts file updated");
        assert_text(&u, "nas.home -> 10.0.0.5");
        assert_eq!(u.kind, LogKind::Info);

        let err = client_event_to_log_entry(&ClientEvent::HostsFileError {
            hostname: "nas.home".into(),
            ip: "10.0.0.5".into(),
            error: "Permission denied".into(),
        });
        assert_text(&err, "Failed to update hosts file");
        assert_text(&err, "Permission denied");
        assert_eq!(err.kind, LogKind::Error);
    }

    #[test]
    fn client_heartbeat_and_parse_error() {
        let h = client_event_to_log_entry(&ClientEvent::HeartbeatReceived);
        assert_text(&h, "Heartbeat");
        assert_eq!(h.kind, LogKind::Info);

        let p = client_event_to_log_entry(&ClientEvent::ParseError {
            raw: "garbage".into(),
            error: "eof".into(),
        });
        assert_text(&p, "Failed to parse");
        assert_text(&p, "garbage");
        assert_eq!(p.kind, LogKind::Warn);
    }
}
