//! dnsless-ui – an egui dashboard for dnsless-homelab.
//!
//! The crate is split into a wasm-safe core and a native shell:
//!
//! * [`state`], [`event`], and [`views`] are platform-agnostic (they depend
//!   only on `egui` + `dnsless_common`), so they can be reused by the
//!   `dnsless-web` WASM UI as well as the native desktop app.
//! * [`app`] and [`net`] are native-only (`std::sync::mpsc`, `std::net`,
//!   `std::thread`): they wire the blocking server/client loops to the
//!   render loop and are compiled out on `wasm32`.
//!
//! See `SPEC-UI.md` at the repository root for the full specification.

pub mod event;
pub mod state;
pub mod views;

#[cfg(not(target_arch = "wasm32"))]
pub mod app;
#[cfg(not(target_arch = "wasm32"))]
pub mod net;

#[cfg(not(target_arch = "wasm32"))]
pub use app::DnslessUiApp;
#[cfg(not(target_arch = "wasm32"))]
pub use net::spawn_network_threads;
