//! Observable UI state, mutated by events drained from the channels.
//!
//! The structs here are owned by [`crate::app::DnslessUiApp`] and read by
//! [`crate::views`].  They contain no `egui` types and perform no I/O, so
//! they are unit-testable without a window or a socket.

use dnsless_client::ClientEvent;
use dnsless_server::config::ServerConfig;
use dnsless_server::ServerEvent;

use crate::event::{client_event_to_log_entry, server_event_to_log_entry};

/// Maximum lines kept in each panel's log history.
///
/// Old entries are dropped (FIFO) once this limit is exceeded, to keep
/// memory bounded on long-running homelab hosts (e.g. Raspberry Pi).
pub const LOG_CAPACITY: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogKind {
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    /// Local timestamp, e.g. `"2026-08-23 23:45:01"`.
    pub timestamp: String,
    pub text: String,
    pub kind: LogKind,
}

#[derive(Debug, Default)]
pub struct ServerState {
    /// `ip:port` the listener bound to, once [`ServerEvent::Listening`] arrives.
    pub bind_addr: Option<String>,
    /// Configured interface name (may be empty = auto-detect).
    pub interface: String,
    /// Configured hostname clients map the server's IP to.
    pub hostname: String,
    /// Current IP of the monitored interface, once known.
    pub current_ip: Option<String>,
    /// Currently connected clients, as `"ip:port"` strings.
    ///
    /// **Never hostnames** — the wire protocol carries no client identity
    /// (see lacuna nº 1 in `SPEC-UI.md`).
    pub connected_clients: Vec<String>,
    /// History of IP changes: `(timestamp, "hostname -> ip")`.
    pub ip_history: Vec<(String, String)>,
    /// Capped log history (see [`LOG_CAPACITY`]).
    pub log: Vec<LogEntry>,
}

#[derive(Debug, Default)]
pub struct ClientState {
    /// Configured `host:port` of the server.
    pub server_addr: Option<String>,
    /// Configured hosts-file path.
    pub hosts_file: String,
    /// Whether the client currently holds a live TCP connection.
    pub connected: bool,
    /// Last applied update `(hostname, ip)`, if any.
    pub last_update: Option<(String, String)>,
    /// History of applied updates: `(timestamp, hostname, ip)`.
    pub update_history: Vec<(String, String, String)>,
    /// Capped log history (see [`LOG_CAPACITY`]).
    pub log: Vec<LogEntry>,
}

impl ServerState {
    pub fn new(cfg: &ServerConfig) -> Self {
        Self {
            bind_addr: None,
            interface: cfg.interface.clone(),
            hostname: cfg.hostname.clone(),
            current_ip: None,
            connected_clients: Vec::new(),
            ip_history: Vec::new(),
            log: Vec::new(),
        }
    }

    /// Apply one server event: mutate structured fields and push a log
    /// entry, truncating the log to [`LOG_CAPACITY`] when it overflows.
    pub fn apply_server_event(&mut self, ev: ServerEvent) {
        let entry = server_event_to_log_entry(&ev);
        match ev {
            ServerEvent::Listening { bind_addr } => {
                self.bind_addr = Some(bind_addr.to_string());
            }
            ServerEvent::ClientConnected { peer } => {
                let peer_str = peer.to_string();
                if !self.connected_clients.contains(&peer_str) {
                    self.connected_clients.push(peer_str);
                }
            }
            ServerEvent::ClientDisconnected { peer } => {
                let peer_str = peer.to_string();
                self.connected_clients.retain(|p| *p != peer_str);
            }
            ServerEvent::IpChanged {
                hostname,
                ip,
                is_initial,
            } => {
                self.current_ip = Some(ip.clone());
                self.ip_history.push((
                    entry.timestamp.clone(),
                    format!(
                        "{} -> {}{}",
                        hostname,
                        ip,
                        if is_initial { " (initial)" } else { "" }
                    ),
                ));
                // Cap ip_history the same way as log, to stay bounded.
                let extra = self.ip_history.len().saturating_sub(LOG_CAPACITY);
                self.ip_history.drain(0..extra);
            }
            ServerEvent::HeartbeatSent
            | ServerEvent::PollUnchanged { .. }
            | ServerEvent::Error { .. } => {}
        }
        self.push_log(entry);
    }

    fn push_log(&mut self, entry: LogEntry) {
        self.log.push(entry);
        let extra = self.log.len().saturating_sub(LOG_CAPACITY);
        self.log.drain(0..extra);
    }
}

impl ClientState {
    pub fn new(cfg: &dnsless_client::config::ClientConfig) -> Self {
        Self {
            server_addr: Some(format!("{}:{}", cfg.server_host, cfg.server_port)),
            hosts_file: cfg.hosts_file.clone(),
            connected: false,
            last_update: None,
            update_history: Vec::new(),
            log: Vec::new(),
        }
    }

    /// Apply one client event: mutate structured fields and push a log
    /// entry, truncating the log to [`LOG_CAPACITY`] when it overflows.
    pub fn apply_client_event(&mut self, ev: ClientEvent) {
        let entry = client_event_to_log_entry(&ev);
        let mut extra_history = false;
        match &ev {
            ClientEvent::Connecting { server_addr } => {
                self.server_addr = Some(server_addr.clone());
                self.connected = false;
            }
            ClientEvent::Connected { server_addr } => {
                self.server_addr = Some(server_addr.clone());
                self.connected = true;
            }
            ClientEvent::ConnectionLost { .. } | ClientEvent::ConnectionFailed { .. } => {
                self.connected = false;
            }
            ClientEvent::IpUpdateReceived { hostname, ip } => {
                self.last_update = Some((hostname.clone(), ip.clone()));
            }
            ClientEvent::HostsFileUpdated { hostname, ip } => {
                self.last_update = Some((hostname.clone(), ip.clone()));
                self.update_history
                    .push((entry.timestamp.clone(), hostname.clone(), ip.clone()));
                extra_history = true;
            }
            ClientEvent::HostsFileError { hostname, ip, .. } => {
                self.last_update = Some((hostname.clone(), ip.clone()));
            }
            ClientEvent::HeartbeatReceived | ClientEvent::ParseError { .. } => {}
        }
        if extra_history {
            let extra = self.update_history.len().saturating_sub(LOG_CAPACITY);
            self.update_history.drain(0..extra);
        }
        self.push_log(entry);
    }

    fn push_log(&mut self, entry: LogEntry) {
        self.log.push(entry);
        let extra = self.log.len().saturating_sub(LOG_CAPACITY);
        self.log.drain(0..extra);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    fn peer(p: u16) -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), p)
    }

    fn server_state() -> ServerState {
        let cfg = ServerConfig::default();
        ServerState::new(&cfg)
    }

    fn client_state() -> ClientState {
        let cfg = dnsless_client::config::ClientConfig::default();
        ClientState::new(&cfg)
    }

    #[test]
    fn server_listen_sets_bind_addr() {
        let mut s = server_state();
        assert!(s.bind_addr.is_none());
        s.apply_server_event(ServerEvent::Listening {
            bind_addr: "0.0.0.0:5353".parse().unwrap(),
        });
        assert_eq!(s.bind_addr.as_deref(), Some("0.0.0.0:5353"));
        assert_eq!(s.log.len(), 1);
    }

    #[test]
    fn server_client_connect_disconnect() {
        let mut s = server_state();
        s.apply_server_event(ServerEvent::ClientConnected { peer: peer(1) });
        s.apply_server_event(ServerEvent::ClientConnected { peer: peer(2) });
        assert_eq!(s.connected_clients.len(), 2);

        // idempotent on duplicate peer
        s.apply_server_event(ServerEvent::ClientConnected { peer: peer(1) });
        assert_eq!(s.connected_clients.len(), 2);

        s.apply_server_event(ServerEvent::ClientDisconnected { peer: peer(1) });
        assert_eq!(s.connected_clients, vec![peer(2).to_string()]);
    }

    #[test]
    fn server_ip_change_updates_fields_and_history() {
        let mut s = server_state();
        s.apply_server_event(ServerEvent::IpChanged {
            hostname: "nas.home".into(),
            ip: "10.0.0.5".into(),
            is_initial: true,
        });
        assert_eq!(s.current_ip.as_deref(), Some("10.0.0.5"));
        assert_eq!(s.ip_history.len(), 1);
        assert!(s.ip_history[0].1.contains("nas.home -> 10.0.0.5"));
        assert!(s.ip_history[0].1.contains("initial"));

        s.apply_server_event(ServerEvent::IpChanged {
            hostname: "nas.home".into(),
            ip: "10.0.0.6".into(),
            is_initial: false,
        });
        assert_eq!(s.current_ip.as_deref(), Some("10.0.0.6"));
        assert_eq!(s.ip_history.len(), 2);
        assert!(!s.ip_history[1].1.contains("initial"));
    }

    #[test]
    fn server_log_truncates_to_capacity() {
        let mut s = server_state();
        for _ in 0..(LOG_CAPACITY + 10) {
            s.apply_server_event(ServerEvent::HeartbeatSent);
        }
        assert_eq!(s.log.len(), LOG_CAPACITY);
    }

    #[test]
    fn client_connect_sets_connected() {
        let mut c = client_state();
        assert!(!c.connected);
        c.apply_client_event(ClientEvent::Connecting {
            server_addr: "127.0.0.1:5353".into(),
        });
        assert!(!c.connected);
        c.apply_client_event(ClientEvent::Connected {
            server_addr: "127.0.0.1:5353".into(),
        });
        assert!(c.connected);
        c.apply_client_event(ClientEvent::ConnectionLost {
            server_addr: "127.0.0.1:5353".into(),
        });
        assert!(!c.connected);
    }

    #[test]
    fn client_connection_failed_clears_connected() {
        let mut c = client_state();
        c.apply_client_event(ClientEvent::Connected {
            server_addr: "127.0.0.1:5353".into(),
        });
        assert!(c.connected);
        c.apply_client_event(ClientEvent::ConnectionFailed {
            server_addr: "127.0.0.1:5353".into(),
            error: "refused".into(),
        });
        assert!(!c.connected);
    }

    #[test]
    fn client_ip_update_updates_last_and_history() {
        let mut c = client_state();
        c.apply_client_event(ClientEvent::IpUpdateReceived {
            hostname: "nas.home".into(),
            ip: "10.0.0.5".into(),
        });
        assert_eq!(c.last_update, Some(("nas.home".into(), "10.0.0.5".into())));
        assert!(
            c.update_history.is_empty(),
            "only HostsFileUpdated goes to history"
        );

        c.apply_client_event(ClientEvent::HostsFileUpdated {
            hostname: "nas.home".into(),
            ip: "10.0.0.5".into(),
        });
        assert_eq!(c.update_history.len(), 1);
        assert_eq!(c.update_history[0].1, "nas.home");
        assert_eq!(c.update_history[0].2, "10.0.0.5");
    }

    #[test]
    fn client_hosts_file_error_sets_last_and_logs_error_kind() {
        let mut c = client_state();
        c.apply_client_event(ClientEvent::HostsFileError {
            hostname: "nas.home".into(),
            ip: "10.0.0.5".into(),
            error: "Permission denied (os error 13)".into(),
        });
        assert_eq!(c.last_update, Some(("nas.home".into(), "10.0.0.5".into())));
        assert!(
            c.update_history.is_empty(),
            "failed updates do not go to history"
        );
        assert_eq!(c.log.last().unwrap().kind, LogKind::Error);
        assert!(c.log.last().unwrap().text.contains("Permission denied"));
    }

    #[test]
    fn client_log_truncates_to_capacity() {
        let mut c = client_state();
        for _ in 0..(LOG_CAPACITY + 10) {
            c.apply_client_event(ClientEvent::HeartbeatReceived);
        }
        assert_eq!(c.log.len(), LOG_CAPACITY);
    }
}
