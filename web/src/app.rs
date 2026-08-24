//! The WASM eframe app: renders the server panel and feeds it from a
//! WebSocket connection to the server's `/events` endpoint.
//!
//! Reuses `dnsless_ui::state::ServerState` and `dnsless_ui::views`, and
//! deserializes `dnsless_common::ServerEvent` from each WS message.

use std::{cell::RefCell, rc::Rc};

use eframe::App;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{MessageEvent, WebSocket};

use dnsless_common::ServerEvent;
use dnsless_ui::state::ServerState;
use dnsless_ui::views;

pub struct DnslessWebApp {
    state: Rc<RefCell<ServerState>>,
}

impl DnslessWebApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let state = Rc::new(RefCell::new(ServerState::new(String::new(), String::new())));
        connect_events(Rc::clone(&state));
        Self { state }
    }
}

impl App for DnslessWebApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.set_visuals(egui::Visuals::dark());
        egui::CentralPanel::default().show(ctx, |ui| {
            let state = self.state.borrow();
            views::server_panel(ctx, ui, &state);
        });
        // Keep the loop alive so WS-driven state changes are rendered promptly.
        ctx.request_repaint();
    }
}

/// Open a WebSocket to `ws://<hostname>:8080/events` (the server's admin
/// WebSocket port; must match `ServerConfig::admin_port`, default 8080).
fn connect_events(state: Rc<RefCell<ServerState>>) {
    let window = web_sys::window().expect("no window");
    let location = window.location();
    let scheme = if location.protocol().map(|p| p == "https:").unwrap_or(false) {
        "wss"
    } else {
        "ws"
    };
    // hostname() omits the port, which is the static-HTTP port (8081); the
    // WebSocket events live on the admin port (8080).
    let hostname = location.hostname().unwrap_or_else(|_| "localhost".into());
    let url = format!("{scheme}://{hostname}:8080/events");

    let ws = match WebSocket::new(&url) {
        Ok(ws) => ws,
        Err(e) => {
            web_sys::console::error_1(&format!("WebSocket::new failed: {e:?}").into());
            return;
        }
    };

    let onmessage_state = Rc::clone(&state);
    let onmessage = Closure::wrap(Box::new(move |e: MessageEvent| {
        if let Some(text) = e.data().as_string() {
            if let Ok(ev) = serde_json::from_str::<ServerEvent>(&text) {
                onmessage_state.borrow_mut().apply_server_event(ev);
            } else {
                web_sys::console::warn_1(&format!("unparseable event: {text}").into());
            }
        }
    }) as Box<dyn FnMut(MessageEvent)>);
    ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
    onmessage.forget();

    let onopen = Closure::wrap(Box::new(move || {
        web_sys::console::log_1(&format!("WebSocket open: {url}").into());
    }) as Box<dyn FnMut()>);
    ws.set_onopen(Some(onopen.as_ref().unchecked_ref()));
    onopen.forget();

    let onerror = Closure::wrap(Box::new(move || {
        web_sys::console::error_1(&"WebSocket error".into());
    }) as Box<dyn FnMut()>);
    ws.set_onerror(Some(onerror.as_ref().unchecked_ref()));
    onerror.forget();
}
