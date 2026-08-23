use dnsless_client::{config::ClientConfig, run};
use std::env;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let config_path = env::args().nth(1).unwrap_or_else(|| "client.toml".into());

    let cfg = ClientConfig::from_file(&config_path).unwrap_or_else(|e| {
        eprintln!("Warning: {e}. Using default configuration.");
        ClientConfig::default()
    });

    run(cfg);
}
