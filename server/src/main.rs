use dnsless_server::{config::ServerConfig, run};
use std::env;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let config_path = env::args().nth(1).unwrap_or_else(|| "server.toml".into());

    let cfg = ServerConfig::from_file(&config_path).unwrap_or_else(|e| {
        eprintln!("Warning: {e}. Using default configuration.");
        ServerConfig::default()
    });

    run(cfg);
}
