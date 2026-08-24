//! Thin binary entry point for dnsless-ui.

use std::env;

use dnsless_client::config::ClientConfig;
use dnsless_server::config::ServerConfig;
use eframe::egui;

use dnsless_ui::app::DnslessUiApp;
use dnsless_ui::net::spawn_network_threads;
use dnsless_ui::state::{ClientState, ServerState};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let server_cfg_path = arg_value("--server-config").unwrap_or_else(|| "server.toml".into());
    let client_cfg_path = arg_value("--client-config").unwrap_or_else(|| "client.toml".into());

    let server_cfg = ServerConfig::from_file(&server_cfg_path).unwrap_or_else(|e| {
        eprintln!("Warning: server config ({server_cfg_path}): {e}. Using default.");
        ServerConfig::default()
    });
    let client_cfg = ClientConfig::from_file(&client_cfg_path).unwrap_or_else(|e| {
        eprintln!("Warning: client config ({client_cfg_path}): {e}. Using default.");
        ClientConfig::default()
    });

    let (server_rx, client_rx) = spawn_network_threads(server_cfg.clone(), client_cfg.clone());
    let server_state = ServerState::new(&server_cfg);
    let client_state = ClientState::new(&client_cfg);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1024.0, 700.0])
            .with_min_inner_size([640.0, 400.0]),
        ..Default::default()
    };

    eframe::run_native(
        "dnsless-homelab UI",
        options,
        Box::new(move |cc| {
            let _ = cc;
            Ok(Box::new(DnslessUiApp::new(
                server_rx,
                client_rx,
                server_state,
                client_state,
            )))
        }),
    )?;
    Ok(())
}

/// Read the value following a `--flag` from argv, if present.
fn arg_value(flag: &str) -> Option<String> {
    let mut args = env::args();
    while let Some(a) = args.next() {
        if a == flag {
            return args.next();
        }
    }
    None
}
