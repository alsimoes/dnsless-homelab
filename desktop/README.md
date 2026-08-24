# dnsless-desktop

A tray-resident desktop app that observes the **client** side of
dnsless-homelab: it spawns the blocking `dnsless_client` loop on a
background thread, feeds `ClientEvent`s into a `ClientState`, and renders
the client panel (`ui::views::client_panel`) in a window that
closes-to-tray. The process stays alive in the system tray until
"Quit" is chosen.

The **server** side of the split UI is the WASM web app served by
`dnsless-server` (see the `web` crate).

## Build

```sh
cargo build --release -p dnsless-desktop
# Binary: target/release/dnsless-desktop (Windows: .exe)
```

## Run

```sh
./target/release/dnsless-desktop --client-config client.toml
```

`--client-config` defaults to `client.toml` in the current directory.

## Platform requirements

- **Windows**: no extra runtime dependencies (uses `Shell_NotifyIconW`).
- **Linux**: requires `gtk3` and `libappindicator` (or
  `libayatana-appindicator`) at runtime. Arch:
  `pacman -S gtk3 libappindicator-gtk3` (or `libayatana-appindicator`).
  Debian/Ubuntu: `sudo apt install libgtk-3-dev libappindicator3-dev`.
  The tray icon is shown via StatusNotifierItem; window managers without
  a status-notifier host (e.g. bare i3) need a tray like `stalonetray`.

## Behaviour

- Tray menu: **Show** / **Hide** toggle the window; **Quit** exits.
- Closing the window (the "X" button) hides to tray instead of quitting,
  so the client keeps watching the server.

## Tests

```sh
cargo test -p dnsless-desktop
```
