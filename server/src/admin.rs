//! Admin HTTP + WebSocket server for the dnsless server.
//!
//! Two listeners, both synchronous (`std::net` + `std::thread` + `tungstenite`
//! in blocking mode) — no async runtime:
//!
//! * `admin_port` (default `8080`) — WebSocket endpoint.  Browsers connect to
//!   `ws://host:8080/events` (the path is ignored; the whole port is the
//!   event stream) and receive [`ServerEvent`]s as JSON.
//! * `admin_port + 1` (default `8081`) — static HTTP, serving the WASM web UI
//!   (`index.html` + `pkg/*`) from the configured assets directory.

use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread,
};

use log::{info, warn};

use dnsless_common::ServerEvent;

type WsClients = Arc<Mutex<Vec<tungstenite::WebSocket<TcpStream>>>>;

/// A broadcast fan-out of [`ServerEvent`]s to connected WebSocket browsers.
pub struct AdminServer {
    clients: WsClients,
}

impl AdminServer {
    /// Bind the admin listeners and spawn their accept loops.
    ///
    /// Returns the handle immediately; if a listener cannot bind it is
    /// skipped (the server keeps running; the web UI is simply unavailable).
    pub fn spawn(admin_port: u16, assets_dir: PathBuf) -> Arc<Self> {
        let srv = Arc::new(Self {
            clients: Arc::new(Mutex::new(Vec::new())),
        });

        // WebSocket events listener.
        match TcpListener::bind(("0.0.0.0", admin_port)) {
            Ok(listener) => {
                info!("Admin WebSocket listening on ws://0.0.0.0:{admin_port}/events");
                let clients = Arc::clone(&srv.clients);
                thread::spawn(move || ws_accept_loop(listener, clients));
            }
            Err(e) => warn!("Cannot bind admin WebSocket on port {admin_port}: {e}"),
        }

        // Static HTTP listener (admin_port + 1).
        let http_port = admin_port.saturating_add(1);
        match TcpListener::bind(("0.0.0.0", http_port)) {
            Ok(listener) => {
                info!("Admin web UI listening on http://0.0.0.0:{http_port}");
                thread::spawn(move || http_accept_loop(listener, assets_dir));
            }
            Err(e) => warn!("Cannot bind admin HTTP on port {http_port}: {e}"),
        }

        srv
    }

    /// Send an event to every connected WebSocket client, dropping any that
    /// have disconnected (a closed browser is not an error).
    pub fn broadcast(&self, ev: &ServerEvent) {
        let Ok(json) = serde_json::to_string(ev) else {
            return;
        };
        let msg = tungstenite::Message::text(json);
        let mut guard = self.clients.lock().unwrap();
        guard.retain_mut(|ws| ws.send(msg.clone()).is_ok());
    }
}

fn ws_accept_loop(listener: TcpListener, clients: WsClients) {
    for incoming in listener.incoming() {
        match incoming {
            Ok(stream) => {
                let c = Arc::clone(&clients);
                thread::spawn(move || match tungstenite::accept(stream) {
                    Ok(ws) => {
                        c.lock().unwrap().push(ws);
                    }
                    Err(e) => warn!("WebSocket handshake failed: {e}"),
                });
            }
            Err(e) => warn!("Admin WebSocket accept error: {e}"),
        }
    }
}

fn http_accept_loop(listener: TcpListener, assets_dir: PathBuf) {
    for incoming in listener.incoming() {
        match incoming {
            Ok(stream) => {
                let d = assets_dir.clone();
                thread::spawn(move || serve_http(stream, d));
            }
            Err(e) => warn!("Admin HTTP accept error: {e}"),
        }
    }
}

/// Minimal synchronous HTTP responder: read the request head, serve one static
/// file, close the connection. Good enough for the web UI assets.
fn serve_http(mut stream: TcpStream, assets_dir: PathBuf) {
    let path = match read_request_path(&mut stream) {
        Some(p) => p,
        None => return,
    };

    let (status, body, mime) = serve_static(&path, &assets_dir);
    let status_line = match status {
        200 => "200 OK",
        403 => "403 Forbidden",
        404 => "404 Not Found",
        _ => "500 Internal Server Error",
    };

    let head = format!(
        "HTTP/1.1 {status_line}\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(&body);
}

/// Read just enough of the HTTP request to extract the path (request line),
/// e.g. `GET /pkg/dnsless_web.js HTTP/1.1` → `/pkg/dnsless_web.js`.
fn read_request_path(stream: &mut TcpStream) -> Option<String> {
    let mut buf = Vec::with_capacity(1024);
    let mut tmp = [0u8; 512];
    loop {
        match stream.read(&mut tmp) {
            Ok(0) => return None,
            Ok(n) => {
                buf.extend_from_slice(&tmp[..n]);
                if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
                if buf.len() > 16 * 1024 {
                    return None;
                }
            }
            Err(_) => return None,
        }
    }
    let head = String::from_utf8_lossy(&buf);
    let request_line = head.lines().next()?;
    let path = request_line.split_whitespace().nth(1).unwrap_or("/");
    Some(path.to_string())
}

/// Serve a static file from the assets directory, mapping `/` to `index.html`
/// and refusing path traversal. Returns `(status, body, mime)`.
fn serve_static(path: &str, assets_dir: &Path) -> (u16, Vec<u8>, String) {
    let rel = path.trim_start_matches('/');
    let rel = if rel.is_empty() { "index.html" } else { rel };

    let root = match assets_dir.canonicalize() {
        Ok(r) => r,
        Err(_) => {
            return (
                404,
                b"assets directory not found".to_vec(),
                "text/plain; charset=utf-8".into(),
            )
        }
    };

    let full = match assets_dir.join(rel).canonicalize() {
        Ok(f) => f,
        Err(_) => {
            return (
                404,
                b"not found".to_vec(),
                "text/plain; charset=utf-8".into(),
            )
        }
    };

    if !full.starts_with(&root) {
        return (
            403,
            b"forbidden".to_vec(),
            "text/plain; charset=utf-8".into(),
        );
    }

    match std::fs::read(&full) {
        Ok(body) => (200, body, mime_for(&full)),
        Err(_) => (
            404,
            b"not found".to_vec(),
            "text/plain; charset=utf-8".into(),
        ),
    }
}

fn mime_for(path: &Path) -> String {
    let mime = match path.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "application/javascript",
        Some("wasm") => "application/wasm",
        Some("css") => "text/css",
        Some("json") => "application/json",
        Some("png") => "image/png",
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        Some("webp") => "image/webp",
        _ => "application/octet-stream",
    };
    mime.to_string()
}
