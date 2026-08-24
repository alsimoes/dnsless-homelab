//! System tray icon for the dnsless-desktop app.
//!
//! Uses the `tray-icon` crate (tauri-apps), cross-platform:
//! Windows (Shell_NotifyIconW), Linux (GTK/StatusNotifierItem via D-Bus),
//! macOS (cocoa). No Tauri runtime required.
//!
//! On Linux, requires `gtk3` and `libappindicator` (or
//! `libayatana-appindicator`) at runtime — see the crate README. The
//! `libxdo` feature is disabled here because we only use plain menu
//! items, not the predefined Cut/Copy/Paste actions.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tray_icon::menu::{IsMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{TrayIcon, TrayIconBuilder};

/// Text ids for the menu items. We match on `MenuEvent::id.0` by string.
const MENU_SHOW: &str = "show";
const MENU_HIDE: &str = "hide";
const MENU_QUIT: &str = "quit";

/// A user action from the tray menu, resolved by the app loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    /// Show the main window.
    Show,
    /// Hide the main window (keep running in the tray).
    Hide,
    /// Quit the whole process.
    Quit,
}

/// Handle to the running tray icon plus the shared quit flag.
///
/// Menu events are drained from the global `MenuEvent::receiver()` channel
/// inside the app's `update()` (see `DnslessDesktopApp::update`), so the
/// eframe event loop stays the single owner of the UI thread.
pub struct Tray {
    /// Set to true when the user picks "Quit" from the tray menu.
    pub quit_flag: Arc<AtomicBool>,
    _icon: TrayIcon,
}

impl Tray {
    /// Build the tray icon, its menu, and the shared quit flag.
    ///
    /// Must be called on the main thread: on Linux this calls
    /// `gtk::init()` first (required before any GTK API), and tray-icon
    /// expects the event loop thread to own the icon.
    pub fn new() -> Self {
        #[cfg(target_os = "linux")]
        gtk::init().expect("failed to initialize GTK for tray icon");

        let quit_flag = Arc::new(AtomicBool::new(false));

        let menu = Menu::new();
        let show = MenuItem::with_id(MENU_SHOW, "Show", true, None);
        let hide = MenuItem::with_id(MENU_HIDE, "Hide", true, None);
        let sep = PredefinedMenuItem::separator();
        let quit = MenuItem::with_id(MENU_QUIT, "Quit", true, None);

        menu.append(&show as &dyn IsMenuItem).expect("append Show");
        menu.append(&hide as &dyn IsMenuItem).expect("append Hide");
        menu.append(&sep as &dyn IsMenuItem)
            .expect("append separator");
        menu.append(&quit as &dyn IsMenuItem).expect("append Quit");

        let icon_bytes = include_bytes!("../assets/tray.png");
        let img = image::load_from_memory(icon_bytes)
            .expect("tray.png must be a valid PNG")
            .to_rgba8();
        let (w, h) = img.dimensions();
        let icon =
            tray_icon::Icon::from_rgba(img.into_raw(), w, h).expect("invalid tray icon rgba");

        let tray = TrayIconBuilder::new()
            .with_tooltip("dnsless-homelab")
            .with_icon(icon)
            .with_menu(Box::new(menu))
            .build()
            .expect("failed to build tray icon");

        Self {
            quit_flag,
            _icon: tray,
        }
    }

    /// Drain pending tray menu events.
    ///
    /// Returns the **last** pending action (menu events are rare; draining
    /// the whole queue and keeping the newest is the least surprising
    /// behaviour). Also folds "Quit" into the shared quit flag so any
    /// other observer sees it.
    pub fn drain_menu_events(&self) -> Option<MenuAction> {
        let receiver = MenuEvent::receiver();
        let mut last: Option<MenuAction> = None;
        while let Ok(ev) = receiver.try_recv() {
            let action = match ev.id.0.as_str() {
                MENU_SHOW => Some(MenuAction::Show),
                MENU_HIDE => Some(MenuAction::Hide),
                MENU_QUIT => Some(MenuAction::Quit),
                _ => None,
            };
            if action == Some(MenuAction::Quit) {
                self.quit_flag.store(true, Ordering::SeqCst);
            }
            if action.is_some() {
                last = action;
            }
        }
        last
    }
}
