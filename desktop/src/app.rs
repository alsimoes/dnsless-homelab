//! The tray-enabled desktop app: renders the client panel and feeds it
//! from the client's background loop.
//!
//! The app observes the **client** side of dnsless-homelab (the machine
//! where this binary runs updates its hosts file). The server panel lives
//! in the WASM web UI served by `dnsless-server`.

use std::sync::mpsc::Receiver;

use dnsless_common::ClientEvent;
use dnsless_ui::state::ClientState;
use dnsless_ui::views;

use crate::tray::{MenuAction, Tray};

/// The desktop app. Owns the tray handle, the client state, and the event
/// receiver populated by the background `dnsless_client` thread.
pub struct DnslessDesktopApp {
    tray: Tray,
    client_state: ClientState,
    client_rx: Receiver<ClientEvent>,
}

impl DnslessDesktopApp {
    pub fn new(tray: Tray, client_rx: Receiver<ClientEvent>, client_state: ClientState) -> Self {
        Self {
            tray,
            client_rx,
            client_state,
        }
    }

    /// Drain every pending client event into the state, without blocking.
    ///
    /// Returns true if at least one event was applied (useful to decide
    /// whether a repaint is required, though we repaint unconditionally in
    /// this app).
    pub fn drain_client(&mut self) -> bool {
        let mut any = false;
        while let Ok(ev) = self.client_rx.try_recv() {
            self.client_state.apply_client_event(ev);
            any = true;
        }
        any
    }

    /// Resolve a tray menu action into viewport commands.
    fn handle_menu(&self, ctx: &egui::Context, action: Option<MenuAction>) {
        match action {
            Some(MenuAction::Show) => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
            Some(MenuAction::Hide) => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            }
            Some(MenuAction::Quit) => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            None => {}
        }
    }
}

impl eframe::App for DnslessDesktopApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.set_visuals(egui::Visuals::dark());

        self.drain_client();

        // Tray menu: Show/Hide toggle window visibility; Quit closes.
        let action = self.tray.drain_menu_events();
        self.handle_menu(ctx, action);

        // Closing the window (the "X" button) hides to tray instead of
        // quitting, so the client keeps running in the background.
        if ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    views::client_panel(ctx, ui, &self.client_state);
                });
        });

        // Keep the loop alive so menu events and background events are
        // reflected promptly.
        ctx.request_repaint();
    }
}

impl Drop for DnslessDesktopApp {
    /// Note on shutdown: the background `dnsless_client` thread runs a
    /// blocking infinite loop and there is no portable way to cancel a
    /// `std::thread` blocked on I/O. When the process exits (window closed
    /// via Quit, or the OS terminates it), the orphan thread dies with it.
    /// The `Sender` drops here, so the next `tx.send` in the loop returns
    /// `Err` — which the loop ignores (same philosophy as the `ui` crate).
    fn drop(&mut self) {
        // Intentionally empty: no joinable handles kept (they would block).
    }
}

/// Convenience accessor used by tests and by the unit test for draining.
#[cfg(test)]
pub fn apply_client_events(state: &mut ClientState, rx: &Receiver<ClientEvent>) -> bool {
    let mut any = false;
    while let Ok(ev) = rx.try_recv() {
        state.apply_client_event(ev);
        any = true;
    }
    any
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn drain_applies_events_to_state() {
        let (tx, rx) = mpsc::channel();
        let mut state = ClientState::new("127.0.0.1:5353".into(), "/etc/hosts".into());

        tx.send(ClientEvent::Connected {
            server_addr: "127.0.0.1:5353".into(),
        })
        .unwrap();

        let applied = apply_client_events(&mut state, &rx);
        assert!(applied, "one event should have been applied");
        assert!(state.connected);
        assert_eq!(state.server_addr.as_deref(), Some("127.0.0.1:5353"));
        assert_eq!(state.log.len(), 1);
    }

    #[test]
    fn drain_empty_returns_false() {
        let (_tx, rx) = mpsc::channel::<ClientEvent>();
        let mut state = ClientState::new("127.0.0.1:5353".into(), "/etc/hosts".into());
        assert!(!apply_client_events(&mut state, &rx));
        assert!(!state.connected);
        assert!(state.log.is_empty());
    }

    #[test]
    fn drain_hosts_file_error_surfaces_in_log() {
        let (tx, rx) = mpsc::channel();
        let mut state = ClientState::new("127.0.0.1:5353".into(), "/etc/hosts".into());

        tx.send(ClientEvent::HostsFileError {
            hostname: "nas.home".into(),
            ip: "10.0.0.5".into(),
            error: "Permission denied".into(),
        })
        .unwrap();

        let applied = apply_client_events(&mut state, &rx);
        assert!(applied);
        assert_eq!(
            state.last_update,
            Some(("nas.home".into(), "10.0.0.5".into()))
        );
        assert_eq!(
            state.log.last().unwrap().kind,
            dnsless_ui::state::LogKind::Error
        );
        assert!(state.log.last().unwrap().text.contains("Permission denied"));
    }
}
