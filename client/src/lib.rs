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

pub use dnsless_common::ClientEvent;

use config::ClientConfig;
use hosts::update_hosts_entry;

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
