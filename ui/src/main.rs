//! Thin binary entry point for dnsless-ui.
//!
//! Native-only: the WASM UI lives in the `dnsless-web` crate, which reuses
//! the `dnsless-ui` *library* (state/views) without this binary.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        native_main()
    }

    #[cfg(target_arch = "wasm32")]
    {
        // Never used on wasm — the web UI is `dnsless-web`.
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn native_main() -> Result<(), Box<dyn std::error::Error>> {
    use dnsless_client::config::ClientConfig;
    use dnsless_server::config::ServerConfig;
    use eframe::egui;

    use dnsless_ui::app::DnslessUiApp;
    use dnsless_ui::net::spawn_network_threads;
    use dnsless_ui::state::{ClientState, ServerState};

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
    let server_state = ServerState::new(server_cfg.hostname.clone(), server_cfg.interface.clone());
    let client_state = ClientState::new(
        format!("{}:{}", client_cfg.server_host, client_cfg.server_port),
        client_cfg.hosts_file.clone(),
    );

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
#[cfg(not(target_arch = "wasm32"))]
fn arg_value(flag: &str) -> Option<String> {
    let mut args = std::env::args();
    while let Some(a) = args.next() {
        if a == flag {
            return args.next();
        }
    }
    None
}
