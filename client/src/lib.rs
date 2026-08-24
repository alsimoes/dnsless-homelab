//! dnsless-client – connects to a dnsless-server, receives IP-update messages,
//! and keeps the local hosts file up to date.

use std::{
    io::{BufRead, BufReader},
    net::TcpStream,
    sync::mpsc::Sender,
    thread,
    time::Duration,
};

use dnsless_common::Message;
use log::{error, info, warn};

pub mod config;
pub mod hosts;

use config::ClientConfig;
use hosts::update_hosts_entry;

/// Observability event emitted by [`run_with_events`] when an event
/// channel is supplied.  Lives here — not in `dnsless_common` — so the
/// wire protocol stays untouched.
#[derive(Debug, Clone)]
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
    /// The hosts file update FAILED (typically: permission denied — see
    /// restrição 6).  The UI must surface this prominently; never swallow it.
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

/// Emit an event on the optional channel.  A closed receiver (UI gone)
/// is not an error worth panicking over: the CLI keeps running.
fn emit(tx: &Option<Sender<ClientEvent>>, ev: ClientEvent) {
    if let Some(tx) = tx {
        let _ = tx.send(ev);
    }
}

fn handle_stream(stream: TcpStream, cfg: &ClientConfig, event_tx: &Option<Sender<ClientEvent>>) {
    let server_addr = format!("{}:{}", cfg.server_host, cfg.server_port);
    let reader = BufReader::new(stream);
    for line in reader.lines() {
        match line {
            Err(e) => {
                warn!("Read error: {e}");
                emit(
                    event_tx,
                    ClientEvent::ConnectionLost {
                        server_addr: server_addr.clone(),
                    },
                );
                break;
            }
            Ok(raw) => {
                let msg: Message = match serde_json::from_str(&raw) {
                    Ok(m) => m,
                    Err(e) => {
                        warn!("Failed to parse message: {e}");
                        emit(
                            event_tx,
                            ClientEvent::ParseError {
                                raw: raw.clone(),
                                error: format!("{e}"),
                            },
                        );
                        continue;
                    }
                };

                match msg {
                    Message::IpUpdate(update) => {
                        info!("Received IP update: {} -> {}", update.hostname, update.ip);
                        emit(
                            event_tx,
                            ClientEvent::IpUpdateReceived {
                                hostname: update.hostname.clone(),
                                ip: update.ip.clone(),
                            },
                        );
                        if let Err(e) =
                            update_hosts_entry(&cfg.hosts_file, &update.hostname, &update.ip)
                        {
                            error!("Failed to update hosts file: {e}");
                            emit(
                                event_tx,
                                ClientEvent::HostsFileError {
                                    hostname: update.hostname.clone(),
                                    ip: update.ip.clone(),
                                    error: format!("{e}"),
                                },
                            );
                        } else {
                            info!("Hosts file updated: {} -> {}", update.hostname, update.ip);
                            emit(
                                event_tx,
                                ClientEvent::HostsFileUpdated {
                                    hostname: update.hostname.clone(),
                                    ip: update.ip.clone(),
                                },
                            );
                        }
                    }
                    Message::Heartbeat => {
                        // No action needed; connection is still alive.
                        emit(event_tx, ClientEvent::HeartbeatReceived);
                    }
                }
            }
        }
    }
}

/// Blocking entry point identical to [`run`], plus an optional event
/// channel.  When `event_tx` is `None`, behaves exactly like `run(cfg)`
/// (logs only).  When `Some`, additionally sends a [`ClientEvent`] per
/// observable occurrence while keeping every existing `log` call.
pub fn run_with_events(cfg: ClientConfig, event_tx: Option<Sender<ClientEvent>>) {
    let server_addr = format!("{}:{}", cfg.server_host, cfg.server_port);
    let reconnect_delay = Duration::from_secs(cfg.reconnect_delay_secs);

    loop {
        info!("Connecting to server at {server_addr}…");
        emit(
            &event_tx,
            ClientEvent::Connecting {
                server_addr: server_addr.clone(),
            },
        );
        match TcpStream::connect(&server_addr) {
            Ok(stream) => {
                info!("Connected.");
                emit(
                    &event_tx,
                    ClientEvent::Connected {
                        server_addr: server_addr.clone(),
                    },
                );
                handle_stream(stream, &cfg, &event_tx);
                warn!(
                    "Connection lost. Reconnecting in {} s…",
                    cfg.reconnect_delay_secs
                );
                emit(
                    &event_tx,
                    ClientEvent::ConnectionLost {
                        server_addr: server_addr.clone(),
                    },
                );
            }
            Err(e) => {
                error!(
                    "Cannot connect to {server_addr}: {e}. Retrying in {} s…",
                    cfg.reconnect_delay_secs
                );
                emit(
                    &event_tx,
                    ClientEvent::ConnectionFailed {
                        server_addr: server_addr.clone(),
                        error: format!("{e}"),
                    },
                );
            }
        }
        thread::sleep(reconnect_delay);
    }
}

/// Backwards-compatible blocking entry point.  Unchanged behaviour.
/// Implemented as `run_with_events(cfg, None)`.
pub fn run(cfg: ClientConfig) {
    run_with_events(cfg, None);
}
