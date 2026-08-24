//! The [`eframe::App`] that owns UI state and drains the event channels.

use std::sync::mpsc::Receiver;

use eframe::App;

use dnsless_client::ClientEvent;
use dnsless_server::ServerEvent;

use crate::state::{ClientState, ServerState};
use crate::views;

/// The egui application. Owns the observable state and the two event
/// receivers populated by background threads (see [`crate::net`]).
pub struct DnslessUiApp {
    pub server_state: ServerState,
    pub client_state: ClientState,
    pub server_rx: Receiver<ServerEvent>,
    pub client_rx: Receiver<ClientEvent>,
}

impl DnslessUiApp {
    pub fn new(
        server_rx: Receiver<ServerEvent>,
        client_rx: Receiver<ClientEvent>,
        server_state: ServerState,
        client_state: ClientState,
    ) -> Self {
        Self {
            server_state,
            client_state,
            server_rx,
            client_rx,
        }
    }

    /// Drain every pending event from both channels without blocking.
    ///
    /// A `Disconnected` receiver is treated the same as `Empty`: we stop
    /// draining.  This can happen if a background thread panicked (it
    /// won't, under normal operation), and the UI should keep rendering
    /// rather than crash.
    fn drain(&mut self) {
        while let Ok(ev) = self.server_rx.try_recv() {
            self.server_state.apply_server_event(ev);
        }
        while let Ok(ev) = self.client_rx.try_recv() {
            self.client_state.apply_client_event(ev);
        }
    }
}

impl App for DnslessUiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Force a dark theme for consistent homelab visuals.
        ctx.set_visuals(egui::Visuals::dark());

        self.drain();

        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.heading("dnsless-homelab UI");
        });

        egui::SidePanel::left("server_panel")
            .resizable(true)
            .default_width(ctx.screen_rect().width() / 2.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        views::server_panel(ctx, ui, &self.server_state);
                    });
            });

        egui::SidePanel::right("client_panel")
            .resizable(true)
            .default_width(ctx.screen_rect().width() / 2.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        views::client_panel(ctx, ui, &self.client_state);
                    });
            });

        // Background threads emit asynchronously; keep the loop alive so
        // newly-arrived events are reflected promptly.
        ctx.request_repaint();
    }
}

impl Drop for DnslessUiApp {
    /// Note on shutdown (see `SPEC-UI.md` §4): the background threads run
    /// blocking infinite loops and there is no portable way to cancel a
    /// `std::thread` blocked on I/O.  When the window closes, eframe
    /// returns from `run_native` and the process exits, taking the
    /// orphan threads with it.  The `Sender`s drop here, so the next
    /// `tx.send` in each loop returns `Err` — which the loops ignore.
    fn drop(&mut self) {
        // Intentionally empty: no joinable handles kept (they would block).
    }
}
