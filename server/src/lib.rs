//! dnsless-server – watches a network interface for IP changes and pushes
//! the new address to all connected clients.

use std::{
    io::{BufRead, BufReader, Write},
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream},
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use dnsless_common::{IpUpdate, Message};
use log::{error, info, warn};

pub mod config;
pub mod ip_detector;

use config::ServerConfig;
use ip_detector::detect_ip;

fn broadcast(clients: &Arc<Mutex<Vec<TcpStream>>>, msg: &Message) {
    let json = match serde_json::to_string(msg) {
        Ok(j) => j,
        Err(e) => {
            error!("Failed to serialize message: {e}");
            return;
        }
    };
    let line = format!("{json}\n");
    let mut guard = clients.lock().unwrap();
    guard.retain_mut(|stream| {
        if stream.write_all(line.as_bytes()).is_err() {
            warn!("Client disconnected, removing from list");
            false
        } else {
            true
        }
    });
}

fn accept_loop(listener: TcpListener, clients: Arc<Mutex<Vec<TcpStream>>>) {
    for incoming in listener.incoming() {
        match incoming {
            Ok(stream) => {
                let peer = stream.peer_addr().unwrap_or(SocketAddr::new(
                    IpAddr::V4(Ipv4Addr::UNSPECIFIED),
                    0,
                ));
                info!("New client connected: {peer}");
                let stream_clone = match stream.try_clone() {
                    Ok(s) => s,
                    Err(e) => {
                        error!("Failed to clone stream: {e}");
                        continue;
                    }
                };
                clients.lock().unwrap().push(stream_clone);

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
            Err(e) => error!("Accept error: {e}"),
        }
    }
}

pub fn run(cfg: ServerConfig) {
    let bind_addr: SocketAddr = format!("0.0.0.0:{}", cfg.port)
        .parse()
        .expect("Invalid bind address");

    let listener = TcpListener::bind(bind_addr).expect("Cannot bind TCP listener");
    info!("Server listening on {bind_addr}");

    let clients: Arc<Mutex<Vec<TcpStream>>> = Arc::new(Mutex::new(Vec::new()));
    let clients_accept = Arc::clone(&clients);

    thread::spawn(move || accept_loop(listener, clients_accept));

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
                    last_ip = Some(ip_str.clone());
                    broadcast(
                        &clients,
                        &Message::IpUpdate(IpUpdate {
                            hostname: cfg.hostname.clone(),
                            ip: ip_str,
                        }),
                    );
                }
            }
            Err(e) => warn!("Could not detect IP for interface '{}': {e}", cfg.interface),
        }

        thread::sleep(poll_interval);
        broadcast(&clients, &Message::Heartbeat);
    }
}
