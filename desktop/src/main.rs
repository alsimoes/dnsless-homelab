//! Thin entry point for dnsless-desktop.
//!
//! A tray-resident app that observes the dnsless **client**: it spawns the
//! blocking client loop on a background thread, feeds `ClientEvent`s into
//! a `ClientState`, and renders `ui::views::client_panel` in a window
//! that closes-to-tray. The process stays alive in the system tray until
//! "Quit" is chosen.
//!
//! Cross-platform: Windows (Shell_NotifyIconW) and Linux (AppIndicator /
//! StatusNotifierItem via GTK).

#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod app;
mod tray;

use std::sync::mpsc;

use dnsless_client::config::ClientConfig;
use dnsless_ui::state::ClientState;

use crate::app::DnslessDesktopApp;
use crate::tray::Tray;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let cfg_path = arg_value("--client-config").unwrap_or_else(|| "client.toml".into());
    let cfg = ClientConfig::from_file(&cfg_path).unwrap_or_else(|e| {
        eprintln!("Warning: client config ({cfg_path}): {e}. Using default.");
        ClientConfig::default()
    });

    // Spawn the client background thread. It runs forever; the UI drains
    // its events once per frame via `try_recv` (see DnslessDesktopApp).
    // `cfg` is cloned for the thread; the original stays for ClientState.
    let (tx, rx) = mpsc::channel();
    let thread_cfg = cfg.clone();
    std::thread::Builder::new()
        .name("dnsless-client".into())
        .spawn(move || {
            dnsless_client::run_with_events(thread_cfg, Some(tx));
        })
        .expect("failed to spawn dnsless-client thread");

    let client_state = ClientState::new(
        format!("{}:{}", cfg.server_host, cfg.server_port),
        cfg.hosts_file.clone(),
    );

    // Build the tray icon before entering the eframe event loop: on
    // Linux, tray-icon's gtk::init() must run on the main thread first.
    // On Windows this is a no-op.
    let tray = Tray::new();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([680.0, 520.0])
            .with_min_inner_size([420.0, 320.0])
            .with_active(true),
        ..Default::default()
    };

    eframe::run_native(
        "dnsless-homelab client",
        options,
        Box::new(move |_cc| Ok(Box::new(DnslessDesktopApp::new(tray, rx, client_state)))),
    )?;

    Ok(())
}

/// Read the value following a `--flag` from argv, if present.
fn arg_value(flag: &str) -> Option<String> {
    let mut args = std::env::args();
    while let Some(a) = args.next() {
        if a == flag {
            return args.next();
        }
    }
    None
}
