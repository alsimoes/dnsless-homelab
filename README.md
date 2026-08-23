# dnsless-homelab

A client/server tool written in **Rust** to cover simple homelabs, such as
virtual machines and Raspberry Pis.

Whenever a server's IP address changes it automatically notifies every
connected client, which then updates the local **hosts** file (`/etc/hosts` on
Linux, `C:\Windows\System32\drivers\etc\hosts` on Windows).  This emulates DNS
behaviour for personal desktops and homelab servers without requiring a real DNS
server.

---

## Architecture

```
┌──────────────────────────────────┐         ┌────────────────────────────────┐
│  Server  (Linux / Windows VM)    │  TCP    │  Client  (Linux / Windows PC)  │
│                                  │ ──────► │                                │
│  • Detects IP changes on NIC     │  5353   │  • Receives IP-update messages │
│  • Pushes updates to clients     │         │  • Updates the hosts file      │
│  • Sends heartbeat periodically  │         │  • Auto-reconnects on failure  │
└──────────────────────────────────┘         └────────────────────────────────┘
```

### Protocol

Messages are newline-delimited JSON sent over a persistent TCP connection.

| Message type | Direction | Purpose |
|---|---|---|
| `ip_update` | server → client | New IP address for a hostname |
| `heartbeat` | server → client | Keep-alive / connection check |

---

## Quick start

### Prerequisites

* [Rust toolchain](https://rustup.rs/) (edition 2021, Rust ≥ 1.70)

### Build

```bash
cargo build --release
# Binaries will be in target/release/
```

### Server setup

1. Copy the example config and edit it:

   ```bash
   cp server/server.toml.example server.toml
   $EDITOR server.toml
   ```

   ```toml
   # server.toml
   hostname = "myserver.home"
   port = 5353
   # interface = "eth0"    # leave blank to auto-detect
   poll_interval_secs = 30
   ```

2. Run the server (pass the config path as first argument, defaults to
   `server.toml` in the current directory):

   ```bash
   ./target/release/dnsless-server server.toml
   ```

   On Windows you may need to allow the port through the firewall:
   ```powershell
   New-NetFirewallRule -DisplayName "dnsless-server" -Direction Inbound -Protocol TCP -LocalPort 5353 -Action Allow
   ```

### Client setup

1. Copy the example config and edit it:

   ```bash
   cp client/client.toml.example client.toml
   $EDITOR client.toml
   ```

   ```toml
   # client.toml
   server_host = "192.168.1.100"   # initial/static IP of your server
   server_port = 5353
   # hosts_file = "/etc/hosts"     # uses platform default when omitted
   reconnect_delay_secs = 10
   ```

2. Run the client (**root / Administrator privileges required** to write the
   hosts file):

   ```bash
   sudo ./target/release/dnsless-client client.toml
   ```

After the first IP-update message the client will add a block like this to
the hosts file:

```
# BEGIN dnsless-homelab: myserver.home
192.168.1.42 myserver.home
# END dnsless-homelab: myserver.home
```

Subsequent updates replace only this block, leaving the rest of the file
untouched.

---

## Running as a service

### Linux (systemd)

**Server** (`/etc/systemd/system/dnsless-server.service`):

```ini
[Unit]
Description=dnsless-homelab server
After=network.target

[Service]
ExecStart=/usr/local/bin/dnsless-server /etc/dnsless/server.toml
Restart=on-failure

[Install]
WantedBy=multi-user.target
```

**Client** (`/etc/systemd/system/dnsless-client.service`):

```ini
[Unit]
Description=dnsless-homelab client
After=network.target

[Service]
ExecStart=/usr/local/bin/dnsless-client /etc/dnsless/client.toml
Restart=on-failure

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl enable --now dnsless-server   # on the server
sudo systemctl enable --now dnsless-client   # on the client
```

### Windows (Task Scheduler / NSSM)

Use [NSSM](https://nssm.cc/) or Windows Task Scheduler to run the binary at
startup as the SYSTEM account (which has permission to modify the hosts file).

---

## Development

```bash
cargo test          # run all tests
cargo clippy        # lint
cargo fmt           # format
```

---

## License

[MIT](LICENSE)
