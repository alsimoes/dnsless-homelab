//! dnsless-web — WASM UI served by dnsless-server.
//!
//! Entry point exported to JavaScript via `#[wasm_bindgen]`. The host
//! HTML page calls `dnsless_web::start()` after the canvas is ready.
//!
//! wasm-only: the whole crate is compiled out on native targets.

#![cfg(target_arch = "wasm32")]

pub mod app;

use wasm_bindgen::prelude::*;

/// Entry point invoked from `index.html`. Bootstraps the eframe
/// `WebRunner` on the provided canvas element.
#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();

    let window = web_sys::window().ok_or("no window")?;
    let document = window.document().ok_or("no document")?;
    let canvas = document
        .get_element_by_id("egui_canvas")
        .ok_or("no #egui_canvas element")?;
    let canvas: web_sys::HtmlCanvasElement = canvas
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .map_err(|_| "#egui_canvas is not a canvas")?;

    wasm_bindgen_futures::spawn_local(async move {
        let runner = eframe::WebRunner::new();
        if let Err(e) = runner
            .start(
                canvas,
                eframe::WebOptions::default(),
                Box::new(|cc| Ok(Box::new(app::DnslessWebApp::new(cc)))),
            )
            .await
        {
            web_sys::console::log_1(&format!("eframe WebRunner failed: {e:?}").into());
        }
    });

    Ok(())
}
