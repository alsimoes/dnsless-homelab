//! dnsless-ui – an egui dashboard for dnsless-homelab.
//!
//! The UI runs entirely on the main thread (the eframe event loop); all
//! network I/O happens on two background threads spawned by [`net::spawn_network_threads`].
//! Events flow back to the UI through `std::sync::mpsc` channels drained
//! non-blockingly (`try_recv`) once per frame, so the render loop never
//! blocks on the network.
//!
//! See `SPEC-UI.md` at the repository root for the full specification.

pub mod app;
pub mod event;
pub mod net;
pub mod state;
pub mod views;

pub use app::DnslessUiApp;
pub use net::spawn_network_threads;
