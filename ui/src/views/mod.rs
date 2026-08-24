//! egui rendering for each panel. Pure reads of state; never mutate.

use egui::{Context, Ui};

use crate::state::{ClientState, LogKind, ServerState};

/// Color for a log kind (dark-theme friendly).
fn color_for(kind: LogKind) -> egui::Color32 {
    match kind {
        LogKind::Info => egui::Color32::from_gray(210),
        LogKind::Warn => egui::Color32::from_rgb(220, 180, 60),
        LogKind::Error => egui::Color32::from_rgb(230, 90, 90),
    }
}

/// Render the server panel. `ctx` is accepted for parity with the spec
/// and for future use (e.g. tooltip requests); the current layout only
/// needs `ui`.
#[allow(clippy::ptr_arg)]
pub fn server_panel(_ctx: &Context, ui: &mut Ui, state: &ServerState) {
    ui.heading("Server");
    ui.separator();

    ui.label(format!("Hostname: {}", state.hostname));
    ui.label(format!(
        "Interface: {}",
        if state.interface.is_empty() {
            "(auto-detect)"
        } else {
            &state.interface
        }
    ));
    ui.label(format!(
        "Listening: {}",
        state.bind_addr.as_deref().unwrap_or("(not yet)")
    ));
    ui.label(format!(
        "Current IP: {}",
        state.current_ip.as_deref().unwrap_or("(unknown)")
    ));
    ui.label(format!(
        "Connected clients: {}",
        state.connected_clients.len()
    ));
    for c in &state.connected_clients {
        ui.label(format!("  • {c}"));
    }

    ui.collapsing("IP history", |ui| {
        if state.ip_history.is_empty() {
            ui.label("(none)");
        }
        for (ts, line) in &state.ip_history {
            ui.label(format!("{ts}  {line}"));
        }
    });

    ui.collapsing("Log", |ui| {
        egui::ScrollArea::vertical()
            .stick_to_bottom(true)
            .max_height(260.0)
            .show(ui, |ui| {
                for entry in &state.log {
                    ui.colored_label(
                        color_for(entry.kind),
                        format!("{}  {}", entry.timestamp, entry.text),
                    );
                }
            });
    });
}

/// Render the client panel.
#[allow(clippy::ptr_arg)]
pub fn client_panel(_ctx: &Context, ui: &mut Ui, state: &ClientState) {
    ui.heading("Client");
    ui.separator();

    ui.label(format!(
        "Server: {}",
        state.server_addr.as_deref().unwrap_or("(not configured)")
    ));
    ui.label(format!("Hosts file: {}", state.hosts_file));
    let (status, color) = if state.connected {
        ("connected", egui::Color32::from_rgb(80, 200, 80))
    } else {
        ("reconnecting", egui::Color32::from_rgb(220, 180, 60))
    };
    ui.colored_label(color, format!("Status: {status}"));
    ui.label(format!(
        "Last update: {}",
        state
            .last_update
            .as_ref()
            .map(|(h, ip)| format!("{h} -> {ip}"))
            .unwrap_or_else(|| "(none)".into())
    ));

    ui.collapsing("Update history", |ui| {
        if state.update_history.is_empty() {
            ui.label("(none)");
        }
        for (ts, h, ip) in &state.update_history {
            ui.label(format!("{ts}  {h} -> {ip}"));
        }
    });

    ui.collapsing("Log", |ui| {
        egui::ScrollArea::vertical()
            .stick_to_bottom(true)
            .max_height(260.0)
            .show(ui, |ui| {
                for entry in &state.log {
                    ui.colored_label(
                        color_for(entry.kind),
                        format!("{}  {}", entry.timestamp, entry.text),
                    );
                }
            });
    });
}
