//! dnsless-client – connects to a dnsless-server, receives IP-update messages,
//! and keeps the local hosts file up to date.

use std::{
    io::{BufRead, BufReader},
    net::TcpStream,
    thread,
    time::Duration,
};

use dnsless_common::Message;
use log::{error, info, warn};

pub mod config;
pub mod hosts;

use config::ClientConfig;
use hosts::update_hosts_entry;

fn handle_stream(stream: TcpStream, cfg: &ClientConfig) {
    let reader = BufReader::new(stream);
    for line in reader.lines() {
        match line {
            Err(e) => {
                warn!("Read error: {e}");
                break;
            }
            Ok(raw) => {
                let msg: Message = match serde_json::from_str(&raw) {
                    Ok(m) => m,
                    Err(e) => {
                        warn!("Failed to parse message: {e}");
                        continue;
                    }
                };

                match msg {
                    Message::IpUpdate(update) => {
                        info!("Received IP update: {} -> {}", update.hostname, update.ip);
                        if let Err(e) =
                            update_hosts_entry(&cfg.hosts_file, &update.hostname, &update.ip)
                        {
                            error!("Failed to update hosts file: {e}");
                        } else {
                            info!(
                                "Hosts file updated: {} -> {}",
                                update.hostname, update.ip
                            );
                        }
                    }
                    Message::Heartbeat => {
                        // No action needed; connection is still alive.
                    }
                }
            }
        }
    }
}

pub fn run(cfg: ClientConfig) {
    let server_addr = format!("{}:{}", cfg.server_host, cfg.server_port);
    let reconnect_delay = Duration::from_secs(cfg.reconnect_delay_secs);

    loop {
        info!("Connecting to server at {server_addr}…");
        match TcpStream::connect(&server_addr) {
            Ok(stream) => {
                info!("Connected.");
                handle_stream(stream, &cfg);
                warn!("Connection lost. Reconnecting in {} s…", cfg.reconnect_delay_secs);
            }
            Err(e) => {
                error!("Cannot connect to {server_addr}: {e}. Retrying in {} s…", cfg.reconnect_delay_secs);
            }
        }
        thread::sleep(reconnect_delay);
    }
}
