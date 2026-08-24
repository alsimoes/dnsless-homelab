//! Spawn the background network threads and hand back the event receivers.
//!
//! This is the only module in the crate that touches
//! [`dnsless_server`] / [`dnsless_client`].  Each background thread calls
//! the blocking `run_with_events` loop forever; the UI drains the
//! returned receivers with `try_recv` once per frame (see
//! [`crate::app::DnslessUiApp`]).

use std::sync::mpsc;
use std::thread;

use dnsless_client::config::ClientConfig;
use dnsless_client::{run_with_events as client_run_with_events, ClientEvent};
use dnsless_server::config::ServerConfig;
use dnsless_server::{run_with_events as server_run_with_events, ServerEvent};

/// Spawn one thread for the server loop and one for the client loop.
///
/// Returns the two receivers the UI will drain with `try_recv`.  The
/// `Sender`s move into the spawned threads; when the UI is dropped the
/// senders drop and the next `tx.send` in each loop returns `Err`, which
/// the loops ignore (see `SPEC-UI.md` §4 "Shutdown").
pub fn spawn_network_threads(
    server_cfg: ServerConfig,
    client_cfg: ClientConfig,
) -> (mpsc::Receiver<ServerEvent>, mpsc::Receiver<ClientEvent>) {
    let (server_tx, server_rx) = mpsc::channel();
    let (client_tx, client_rx) = mpsc::channel();

    thread::Builder::new()
        .name("dnsless-server".into())
        .spawn(move || server_run_with_events(server_cfg, Some(server_tx)))
        .expect("failed to spawn dnsless-server thread");

    thread::Builder::new()
        .name("dnsless-client".into())
        .spawn(move || client_run_with_events(client_cfg, Some(client_tx)))
        .expect("failed to spawn dnsless-client thread");

    (server_rx, client_rx)
}
