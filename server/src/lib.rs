//! dnsless-server – watches a network interface for IP changes and pushes
//! the new address to all connected clients.

use std::{
    io::{BufRead, BufReader, Write},
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream},
    sync::{mpsc::Sender, Arc, Mutex},
    thread,
    time::Duration,
};

use dnsless_common::{IpUpdate, Message};
use log::{error, info, warn};

pub mod config;
pub mod ip_detector;

use config::ServerConfig;
use ip_detector::detect_ip;

/// A connected client as the server tracks it internally.
///
/// Carries the peer's socket address so that disconnections can be
/// reported by `SocketAddr` (the wire protocol carries no hostname —
/// see lacuna nº 1).  This struct is private to the module.
struct ClientConn {
    peer: SocketAddr,
    stream: TcpStream,
}

/// Observability event emitted by [`run_with_events`] when an event
/// channel is supplied.  Lives here — not in `dnsless_common` — so the
/// wire protocol stays untouched.
///
/// Clients are identified only by [`SocketAddr`]; a "list of clients by
/// hostname" does not exist in the current protocol (see lacuna nº 1).
#[derive(Debug, Clone)]
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

/// Emit an event on the optional channel.  A closed receiver (UI gone)
/// is not an error worth panicking over: the CLI keeps running.
fn emit(tx: &Option<Sender<ServerEvent>>, ev: ServerEvent) {
    if let Some(tx) = tx {
        let _ = tx.send(ev);
    }
}

fn broadcast(
    clients: &Arc<Mutex<Vec<ClientConn>>>,
    msg: &Message,
    event_tx: &Option<Sender<ServerEvent>>,
) {
    let json = match serde_json::to_string(msg) {
        Ok(j) => j,
        Err(e) => {
            error!("Failed to serialize message: {e}");
            return;
        }
    };
    let line = format!("{json}\n");
    let mut guard = clients.lock().unwrap();
    guard.retain_mut(|conn| {
        if conn.stream.write_all(line.as_bytes()).is_err() {
            warn!("Client disconnected, removing from list");
            emit(
                event_tx,
                ServerEvent::ClientDisconnected { peer: conn.peer },
            );
            false
        } else {
            true
        }
    });
}

fn accept_loop(
    listener: TcpListener,
    clients: Arc<Mutex<Vec<ClientConn>>>,
    event_tx: Option<Sender<ServerEvent>>,
) {
    for incoming in listener.incoming() {
        match incoming {
            Ok(stream) => {
                let peer = stream
                    .peer_addr()
                    .unwrap_or(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0));
                info!("New client connected: {peer}");
                let stream_clone = match stream.try_clone() {
                    Ok(s) => s,
                    Err(e) => {
                        error!("Failed to clone stream: {e}");
                        continue;
                    }
                };
                clients.lock().unwrap().push(ClientConn {
                    peer,
                    stream: stream_clone,
                });
                emit(&event_tx, ServerEvent::ClientConnected { peer });

                // Spawn a reader thread so we can detect client disconnection.
                thread::spawn(move || {
                    let mut reader = BufReader::new(stream);
                    let mut line = String::new();
                    loop {
                        line.clear();
                        match reader.read_line(&mut line) {
                            Ok(0) => {
                                info!("Client {peer} disconnected");
                                break;
                            }
                            Err(e) => {
                                warn!("Read error from {peer}: {e}");
                                break;
                            }
                            _ => {}
                        }
                    }
                });
            }
            Err(e) => {
                error!("Accept error: {e}");
                emit(
                    &event_tx,
                    ServerEvent::Error {
                        message: format!("Accept error: {e}"),
                    },
                );
            }
        }
    }
}

/// Blocking entry point identical to [`run`], plus an optional event
/// channel.  When `event_tx` is `None`, behaves exactly like `run(cfg)`
/// (logs only).  When `Some`, additionally sends a [`ServerEvent`] per
/// observable occurrence while keeping every existing `log` call.
pub fn run_with_events(cfg: ServerConfig, event_tx: Option<Sender<ServerEvent>>) {
    let bind_addr: SocketAddr = format!("0.0.0.0:{}", cfg.port)
        .parse()
        .expect("Invalid bind address");

    let listener = TcpListener::bind(bind_addr).expect("Cannot bind TCP listener");
    info!("Server listening on {bind_addr}");
    emit(&event_tx, ServerEvent::Listening { bind_addr });

    let clients: Arc<Mutex<Vec<ClientConn>>> = Arc::new(Mutex::new(Vec::new()));
    let clients_accept = Arc::clone(&clients);
    let accept_event_tx = event_tx.clone();

    thread::spawn(move || accept_loop(listener, clients_accept, accept_event_tx));

    let mut last_ip: Option<String> = None;
    let poll_interval = Duration::from_secs(cfg.poll_interval_secs);

    loop {
        match detect_ip(&cfg.interface) {
            Ok(ip) => {
                let ip_str = ip.to_string();
                if last_ip.as_deref() != Some(&ip_str) {
                    if last_ip.is_some() {
                        info!("IP changed to {ip_str}, notifying clients");
                    } else {
                        info!("Initial IP: {ip_str}");
                    }
                    let is_initial = last_ip.is_none();
                    last_ip = Some(ip_str.clone());
                    emit(
                        &event_tx,
                        ServerEvent::IpChanged {
                            hostname: cfg.hostname.clone(),
                            ip: ip_str.clone(),
                            is_initial,
                        },
                    );
                    broadcast(
                        &clients,
                        &Message::IpUpdate(IpUpdate {
                            hostname: cfg.hostname.clone(),
                            ip: ip_str,
                        }),
                        &event_tx,
                    );
                } else {
                    emit(&event_tx, ServerEvent::PollUnchanged { ip: ip_str });
                }
            }
            Err(e) => {
                warn!("Could not detect IP for interface '{}': {e}", cfg.interface);
                emit(
                    &event_tx,
                    ServerEvent::Error {
                        message: format!(
                            "Could not detect IP for interface '{}': {e}",
                            cfg.interface
                        ),
                    },
                );
            }
        }

        thread::sleep(poll_interval);
        broadcast(&clients, &Message::Heartbeat, &event_tx);
        emit(&event_tx, ServerEvent::HeartbeatSent);
    }
}

/// Backwards-compatible blocking entry point.  Unchanged behaviour.
/// Implemented as `run_with_events(cfg, None)`.
pub fn run(cfg: ServerConfig) {
    run_with_events(cfg, None);
}
