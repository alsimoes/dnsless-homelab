# SPEC-UI.md — Crate `ui` para dnsless-homelab

> Documento de especificação técnica produzido pelo **Arquiteto (Agente X / GLM-5.2)**.
> O **Implementador (Agente Y / Deepseek)** deve seguir este documento à risca:
> qualquer ambiguidade deve ser sinalizada, jamais resolvida por suposição.

Decisões humanas já tomadas (referência rápida para o Implementador):
- **egui/eframe = "0.29"** (versão pinada no `Cargo.toml` do crate `ui`).
- **Modo da UI**: dois painéis lado a lado, simultâneos (server + client).
- **Origem das configs**: reutilizar `server.toml` e `client.toml` existentes
  via `ServerConfig::from_file` / `ClientConfig::from_file` já existentes.
- **Escopo do refactor**: refatorar `server/src/lib.rs` e `client/src/lib.rs`
  adicionando `run_with_events(cfg, tx)` mantendo `run(cfg)` como wrapper
  que chama `run_with_events(cfg, None)`.

---

## 1. Escopo

### Entra nesta iteração

- Novo crate membro do workspace `ui` (pacote `dnsless-ui`, binário `dnsless-ui`).
- Interface gráfica em **egui/eframe 0.29**, dois painéis simultâneos
  (Servidor à esquerda, Cliente à direita).
- Refatoração mínima e reversível de observabilidade em `server/src/lib.rs`
  e `client/src/lib.rs`: nova função `run_with_events(cfg, tx) -> !` que
  emite eventos por canal `std::sync::mpsc::Sender` opcional; `run(cfg)`
  passa a ser um wrapper que chama `run_with_events(cfg, None)`.
- Dois enums de evento novos — `ServerEvent` e `ClientEvent` — **dentro do
  crate `ui`** (não em `common`, para não tocar no protocolo).
- Módulos do crate `ui`: `event`, `net`, `state`, `views`, `app` (ver seção 5).
- Testes unitários sobre parsing/transformação de eventos e estado da UI.
- Atualização do `Cargo.toml` raiz para incluir `"ui"` em `members`.

### Fica explicitamente FORA de escopo

- **Extensão de protocolo** (novas variantes em `Message`, ex. mensagem de
  identificação de cliente por hostname). Lacuna nº 1 NÃO é resolvida aqui.
  A UI lista clientes conectados **apenas por `SocketAddr`** (IP:porta TCP),
  exatamente como o servidor já conhece hoje. Ver restrição 5.
- **Novo runtime assíncrono** (`tokio`/`async-std`). Toda concorrência usa
  `std::thread` + `std::sync::mpsc`, coerente com `server`/`client` atuais.
- Persistência de estado da UI entre execuções (sem `persistence` feature).
- Temas customizados/cores próprias (usa o tema padrão do egui).
- Internacionalização (textos em inglês, coerente com o resto do projeto).
- Escrita no hosts file a partir da UI (a UI apenas **observa** o cliente;
  quem escreve é `dnsless_client::run_with_events`). A UI exibe erros de
  permissão recebidos via `ClientEvent`, nunca tenta escrever diretamente.

---

## 2. Decisão de observabilidade (lacuna nº 2)

### Princípio

`dnsless_server::run(cfg)` e `dnsless_client::run(cfg)` são **bloqueantes e
infinitas** e hoje só emitem via macro `log`. A UI precisa de eventos em
tempo real. A solução é **adicionar** uma função irmã `run_with_events` que
recebe um `Option<mpsc::Sender<...>>`. Quando `None`, comportamento
idêntico ao `run` atual (somente `log`). Quando `Some(tx)`, emite também
eventos estruturados no canal (e mantém os `log`, para não regredir
observabilidade dos binários CLI).

`run(cfg)` é preservado como wrapper fino — **não muda de comportamento**,
cumprindo a restrição 1 e 2. Os binários CLI existentes (`server/src/main.rs`,
`client/src/main.rs`) **não são alterados**.

### Assinaturas exatas

Em `server/src/lib.rs` (adicionar; não remover nada existente):

```rust
use std::sync::mpsc::Sender;

/// Event emitted by the server's background loop, surfaced to the UI.
/// (Defined here — NOT in dnsless_common — to keep the wire protocol untouched.)
pub enum ServerEvent {
    /// The TCP listener bound successfully and is accepting connections.
    Listening { bind_addr: SocketAddr },
    /// A client connected (identified only by socket address — see lacuna nº 1).
    ClientConnected { peer: SocketAddr },
    /// A client disconnected or was removed during broadcast.
    ClientDisconnected { peer: SocketAddr },
    /// The monitored interface's IP changed (or was detected the first time).
    IpChanged { hostname: String, ip: String, is_initial: bool },
    /// A heartbeat was broadcast to all clients.
    HeartbeatSent,
    /// A non-fatal operational error (accept failure, IP detection failure, ...).
    Error { message: String },
    /// The poll loop ticked and the IP was unchanged (informational).
    PollUnchanged { ip: String },
}

/// Blocking entry point identical to `run`, plus an optional event channel.
/// When `event_tx` is `None`, behaves exactly like `run(cfg)` (logs only).
/// When `Some`, additionally sends a `ServerEvent` per observable occurrence.
pub fn run_with_events(cfg: ServerConfig, event_tx: Option<Sender<ServerEvent>>) {
    // ... corpo refatorado de run(), mantendo TODOS os log::info/warn/error existentes ...
}

/// Backwards-compatible blocking entry point. Unchanged behaviour.
/// Implemented as: run_with_events(cfg, None)
pub fn run(cfg: ServerConfig) {
    run_with_events(cfg, None);
}
```

Em `client/src/lib.rs` (adicionar; não remover nada existente):

```rust
use std::sync::mpsc::Sender;

/// Event emitted by the client's background loop, surfaced to the UI.
pub enum ClientEvent {
    /// Attempting to connect to the configured server.
    Connecting { server_addr: String },
    /// TCP connection to the server succeeded.
    Connected { server_addr: String },
    /// Connection lost (read error or EOF); will reconnect after delay.
    ConnectionLost { server_addr: String },
    /// Connection attempt failed; will retry after delay.
    ConnectionFailed { server_addr: String, error: String },
    /// An IP-update message was received from the server.
    IpUpdateReceived { hostname: String, ip: String },
    /// The hosts file was successfully updated.
    HostsFileUpdated { hostname: String, ip: String },
    /// The hosts file update FAILED (typically: permission denied — see restrição 6).
    /// The UI must surface this prominently; never swallow it.
    HostsFileError { hostname: String, ip: String, error: String },
    /// A heartbeat was received (connection still alive).
    HeartbeatReceived,
    /// A message from the server could not be parsed.
    ParseError { raw: String, error: String },
}

/// Blocking entry point identical to `run`, plus an optional event channel.
/// When `event_tx` is `None`, behaves exactly like `run(cfg)` (logs only).
pub fn run_with_events(cfg: ClientConfig, event_tx: Option<Sender<ClientEvent>>) {
    // ... corpo refatorado de run(), mantendo TODOS os log::info/warn/error existentes ...
}

/// Backwards-compatible blocking entry point. Unchanged behaviour.
pub fn run(cfg: ClientConfig) {
    run_with_events(cfg, None);
}
```

### Regras de refatoração (não-negociáveis para o Implementador)

1. **Nenhum `log::` existente pode ser removido.** Os binários CLI dependem
   deles. O Implementador duplica: `info!(...)` continua, e logo em seguida
   (quando aplicável) `let _ = event_tx.as_ref().map(|tx| tx.send(...));`
   Erros de `send` (rx fechado) são ignorados silenciosamente (canal fechado
   significa UI fechou; o loop deve continuar rodando para o CLI).
2. **`ServerEvent` e `ClientEvent` moram em `server/src/lib.rs` e
   `client/src/lib.rs` respectivamente** — exportados como `pub enum`. O
   crate `ui` os importa via `dnsless_server::ServerEvent` e
   `dnsless_client::ClientEvent`. **Não duplicar em `ui`.**
3. **`common/src/lib.rs` não é tocado** (restrição 5). Os enums de evento
   são observabilidade local, não protocolo de rede.
4. O `broadcast` do servidor hoje remove clientes cujo `write_all` falha;
   o evento `ClientDisconnected` deve ser emitido para cada um desses
   removidos (com o `peer` que estava associado ao stream — ver observação
   de mapeamento abaixo).
5. Mapeamento `SocketAddr ↔ TcpStream`: hoje a thread de leitura do server
   conhece o `peer` mas não tem acesso ao `Vec<TcpStream>` para removê-lo
   ao detectar EOF; o `broadcast` remove por falha de escrita (não por
   EOF). **Manter esse comportamento**: a desconexão é reportada
   exclusivamente via `broadcast` (falha de escrita), **não** via a thread
   de leitura. A thread de leitura também não emite eventos (continua só
   fazendo `log`). Isso evita alterar a topologia de threads do servidor.
   Consequência documentada: um cliente que conecta mas nunca recebe
   broadcast (ex. servidor sem mudança de IP por muito tempo) pode
   permanecer na lista mesmo após desconectar — bug pré-existente, fora
   de escopo, apenas anotado.
6. `run_with_events` permanece `-> ` implícito (não retorna; é um loop
   infinito). **Não adicionar** `-> !` (never type instável em stable até
   a toolchain do projeto; `run` atual também não usa).
7. O wrapper `run(cfg)` deve ser **exatamente** `run_with_events(cfg, None)`
   — nenhuma lógica extra.

---

## 3. Modelo de dados de eventos

Já definido na seção 2. Resumo canônico para o Implementador (copiar
fielmente os nomes de variantes e campos):

`ServerEvent` variantes:
- `Listening { bind_addr: SocketAddr }`
- `ClientConnected { peer: SocketAddr }`
- `ClientDisconnected { peer: SocketAddr }`
- `IpChanged { hostname: String, ip: String, is_initial: bool }`
- `HeartbeatSent`
- `Error { message: String }`
- `PollUnchanged { ip: String }`

`ClientEvent` variantes:
- `Connecting { server_addr: String }`
- `Connected { server_addr: String }`
- `ConnectionLost { server_addr: String }`
- `ConnectionFailed { server_addr: String, error: String }`
- `IpUpdateReceived { hostname: String, ip: String }`
- `HostsFileUpdated { hostname: String, ip: String }`
- `HostsFileError { hostname: String, ip: String, error: String }`
- `HeartbeatReceived`
- `ParseError { raw: String, error: String }`

**Derives**: `#[derive(Debug, Clone)]` em ambos. **Não** `Serialize`/
`Deserialize` (eventos nunca cruzam a rede). **Não** `PartialEq` exigido
(exceto se útil para teste — opcional, deixar o Implementador adicionar
`PartialEq` onde ajudar os testes, com `String` comparável por valor).

**Mapeamento de ocorrências → evento** (tabela de verdade para o Implementador):

| Ocorrência no código atual | Evento a emitir (quando `tx` presente) |
|---|---|
| `info!("Server listening on {bind_addr}")` | `Listening { bind_addr }` |
| `info!("New client connected: {peer}")` | `ClientConnected { peer }` |
| `warn!("Client disconnected, removing from list")` (em `broadcast`) | `ClientDisconnected { peer }` — **cuidado**: `broadcast` não tem o `peer` hoje; ver nota abaixo |
| `info!("Initial IP: {ip_str}")` | `IpChanged { hostname, ip, is_initial: true }` |
| `info!("IP changed to {ip_str}, notifying clients")` | `IpChanged { hostname, ip, is_initial: false }` |
| `broadcast(&clients, &Message::Heartbeat)` após sleep | `HeartbeatSent` |
| `Err(e) => warn!("Could not detect IP ...")` | `Error { message }` |
| (novo) laço detecta IP igual ao `last_ip` | `PollUnchanged { ip }` (novo; hoje é implícito) |
| `error!("Accept error: {e}")` | `Error { message }` |
| `info!("Connecting to server at {server_addr}…")` (client) | `Connecting { server_addr }` |
| `info!("Connected.")` (client) | `Connected { server_addr }` |
| `warn!("Connection lost. Reconnecting ...")` (client) | `ConnectionLost { server_addr }` |
| `error!("Cannot connect to {server_addr}: {e}...")` (client) | `ConnectionFailed { server_addr, error }` |
| `info!("Received IP update: {} -> {}", h, ip)` | `IpUpdateReceived { hostname, ip }` |
| `info!("Hosts file updated: ...")` | `HostsFileUpdated { hostname, ip }` |
| `error!("Failed to update hosts file: {e}")` | `HostsFileError { hostname, ip, error }` |
| `Message::Heartbeat` no client (hoje: noop) | `HeartbeatReceived` |
| `warn!("Failed to parse message: {e}")` | `ParseError { raw, error }` |

**Nota sobre `ClientDisconnected` em `broadcast`**: a função `broadcast`
atual itera `guard.retain_mut` sobre `Vec<TcpStream>` e descarta quem falha
em `write_all`, mas **não tem o `SocketAddr`** (streams não carregam o
peer atrelado de forma estática após `try_clone`). Para emitir
`ClientDisconnected` com `peer`, o Implementador deve **trocar o tipo do
vetor compartilhado** de `Arc<Mutex<Vec<TcpStream>>>` para
`Arc<Mutex<Vec<ClientConn>>>` onde:

```rust
/// A connected client as the server tracks it.
struct ClientConn {
    peer: SocketAddr,
    stream: TcpStream,
}
```

Esta struct é **privada** ao módulo `server/src/lib.rs` (não exportada).
`accept_loop` popula `peer` com `stream.peer_addr()` (mantendo o fallback
`SocketAddr::new(Ipv4Addr::UNSPECIFIED, 0)` já existente). Em `broadcast`,
`retain_mut` emite `ClientDisconnected { peer: conn.peer }` antes de
descartar. **Esta é a única alteração estrutural interna permitida em
`server/src/lib.rs` além de adicionar `run_with_events`.**

---

## 4. Modelo de concorrência

### Topologia

```
┌────────────────────────────────────────────────────────────────────┐
│ Main thread (eframe event loop, nunca bloqueia em I/O de rede)    │
│  ┌──────────────────────────────────────────────────────────────┐  │
│  │ DnslessUiApp::update(&mut self, ctx, _frame)                 │  │
│  │   - server_rx.try_recv() -> aplica a server_state            │  │
│  │   - client_rx.try_recv() -> aplica a client_state            │  │
│  │   - renderiza painéis via views::server_panel / client_panel │  │
│  │   - se houver eventos: ctx.request_repaint()                 │  │
│  └──────────────────────────────────────────────────────────────┘  │
└──────────────▲───────────────────────────────────▲─────────────────┘
               │ mpsc::Receiver<ServerEvent>       │ mpsc::Receiver<ClientEvent>
               │                                   │
   ┌───────────┴──────────────┐        ┌───────────┴──────────────┐
   │ thread "dnsless-server"  │        │ thread "dnsless-client"  │
   │  dnsless_server::        │        │  dnsless_client::        │
   │    run_with_events(scfg, │        │    run_with_events(ccfg, │
   │      Some(server_tx))    │        │      Some(client_tx))    │
   │  (bloqueia para sempre)  │        │  (bloqueia para sempre) │
   └──────────────────────────┘        └──────────────────────────┘
```

- Canal: `std::sync::mpsc::channel` — **um por lado** (server e client).
  `Sender` movido para a thread de fundo; `Receiver` guardado na `App`.
- A UI lê via `try_recv()` (não bloqueante) a cada `update`. Se houver
  pelo menos um evento em qualquer canal, chamar `ctx.request_repaint()`
  ao final para garantir continuidade. (egui já re-renderiza a 60 fps por
  padrão quando há interação, mas eventos assíncronos exigem o
  `request_repaint` explícito para não ficar parado.)
- **Não usar `poll_recv`** (instável). Usar `try_recv` em loop até
  `Err(TryRecvError::Empty)` drenando todos os eventos pendentes por frame.
- Limite de backlog: o canal é `unbounded` (default do `mpsc::channel`).
  Histórico de eventos exibido é **truncado** no estado da UI a um máximo
  de **200 linhas de log** (configurável como constante `LOG_CAPACITY`
  em `state.rs`, valor `200`). Itens antigos são descartados do vetor
  de log quando estoura. Isto evita crescimento de memória indefinido em
  execuções longas (importante em Raspberry Pi).

### Shutdown (o que acontece quando a janela fecha)

- eframe chama `drop` na `App` quando a janela é fechada. Em `Drop` para
  `DnslessUiApp`, **não há** maneira portable de matar `std::thread::spawn`
  bloqueante em I/O (não há `cancel`). As threads de fundo ficarão vivas
  até o **processo inteiro terminar** — e como a main thread é a event
  loop do eframe, ao retornar de `eframe::run_native` o processo faz
  exit e as threads de fundo são finalizadas pelo SO (não há `JoinHandle`
  bloqueante; o eframe retoma controle e o processo termina).
- **Implementação**: a `App` guarda `Option<JoinHandle>` para cada thread?
  **Não**. As threads de fundo chamam loops infinitos e nunca terminam
  naturalmente; guardar o handle e dar `.join()` no `Drop` travaria o
  encerramento. Portanto: **as threads ficam órfãs e o processo termina
  no exit do eframe**, que é o comportamento esperado e aceitável para um
  app de homelab monolítico. Documentar isso em comentário no `app.rs`.
- O `Sender` cai de escopo quando a `App` é dropada → o próximo
  `tx.send(...)` na thread de fundo retorna `Err(SendError)` → a thread
  de fundo deve **ignorar** o erro (não panicar, não logar spam). Já
  especificado na seção 2 regra 1.
- **Não usar** `Arc<AtomicBool>` de shutdown propalado a matar o loop:
  os loops fazem `thread::sleep` e I/O bloqueante; um flag não
  interrompe prontamente. Manter simples: processo termina, threads
  morrem com ele.

### Restrição de não-bloqueio da UI (restrição 3)

A `update` da `App` **nunca** chama nada de rede. Tudo que ela faz é:
`try_recv` (não-bloqueante), mutar structs de estado, chamar métodos de
render do egui. Conformidade verificável: ausência de `use` de
`std::net::*`, `dnsless_server::run*`, `dnsless_client::run*` em
`app.rs`/`state.rs`/`views.rs`. Apenas em `net.rs`.

---

## 5. Divisão de módulos no crate `ui`

Estrutura de arquivos:

```
ui/
├── Cargo.toml
├── src/
│   ├── main.rs          # binário fino: parse args, carrega configs, sobe app
│   ├── lib.rs           # re-export público + declaração dos submódulos
│   ├── event.rs         # conversão ServerEvent/ClientEvent -> LogEntry (puro, testável)
│   ├── net.rs           # spawn das threads de fundo; dona dos Senders/Receivers
│   ├── state.rs         # ServerState, ClientState, LogEntry, LOG_CAPACITY
│   ├── views/
│   │   ├── mod.rs       # server_panel(ctx, ui, state) / client_panel(ctx, ui, state)
│   │   └── (arquivo único; se crescer, separar depois)
│   └── app.rs           # DnslessUiApp: eframe::App + Drop
```

### Responsabilidades (responsabilidade única por módulo)

- **`event.rs`** — funções puras de **transformação**:
  `server_event_to_log_entry(&ServerEvent) -> LogEntry` e
  `client_event_to_log_entry(&ClientEvent) -> LogEntry`. Também extrai
  campos estruturados para os painéis (ex.: "qual IP novo"). **Puro, sem
  I/O, sem egui.** Alvo principal de testes unitários.
- **`net.rs`** — única camada que toca `dnsless_server`/`dnsless_client`.
  Função `spawn_network_threads(scfg: ServerConfig, ccfg: ClientConfig)
  -> (Receiver<ServerEvent>, Receiver<ClientEvent>)` que cria dois canais
  `mpsc`, dá `spawn` em duas threads nomeadas e retorna os `Receiver`.
  **Sem egui.** Não é testado por unit test (exige socket real) — fora do
  escopo de testes.
- **`state.rs`** — structs de estado observadas pelas views:
  `LogEntry { timestamp: String, text: String, kind: LogKind }`,
  `LogKind { enum Info|Warn|Error }`, `ServerState { ... }`,
  `ClientState { ... }`. Métodos `apply_server_event(&mut self, ev)` e
  `apply_client_event(&mut self, ev)` que mutam o estado e empurram uma
  `LogEntry` no histórico (com truncagem para `LOG_CAPACITY`). **Sem
  I/O, sem egui.** Alvo de testes unitários.
- **`views/mod.rs`** — funções de render egui puras:
  `pub fn server_panel(ctx: &egui::Context, ui: &mut egui::Ui, state: &ServerState)`
  e análoga `client_panel`. **Não mutam estado** (recebem `&`), só leem.
  Sem testes unitários (render é difícil de testar sem egui).
- **`app.rs`** — `pub struct DnslessUiApp` implementando `eframe::App`.
  Guarda `server_state: ServerState`, `client_state: ClientState`,
  `server_rx: Receiver<ServerEvent>`, `client_rx: Receiver<ClientEvent>`.
  `update` drena os dois canais via `try_recv` e chama
  `apply_server_event`/`apply_client_event`, depois `views::server_panel`
  e `views::client_panel` em `SidePanel::left` e `SidePanel::right`
  (ou `ui.columns(2, ...)` — ver nota de layout). Implementa `Drop` com
  comentário documentando o shutdown "órfão". Sem testes unitários do
  trait `App` (exige eframe real); mas `new()` e a lógica de drenagem
  extraída podem ser testadas se o Implementador achar útil.
- **`main.rs`** — binário fino: parse de args (`--server-config`,
  `--client-config`, ambos com default), `ServerConfig::from_file` e
  `ClientConfig::from_file`, `spawn_network_threads`, monta
  `DnslessUiApp` e chama `eframe::run_native`. **Sem lógica de negócio.**
- **`lib.rs`** — declara `pub mod event; pub mod net; pub mod state;
  pub mod views; pub mod app;` e re-exporta `DnslessUiApp` e
  `spawn_network_threads` para eventual uso como lib (não exigido, mas
  coerente com o padrão `server`/`client` que têm `lib.rs` + `main.rs`).

### Nota de layout (decisão já tomada)

Usar `egui::SidePanel::left("server_panel")` e `egui::SidePanel::right("client_panel")`
com `resizable: true` e largura inicial proporcional. Cabeçalho do app
(`egui::TopBottomPanel::top`) com título "dnsless-homelab UI". Layout
responsivo: cada painel tem seu próprio `ScrollArea` vertical para o log.

---

## 6. Contratos de interface (assinaturas prontas para implementação)

### `ui/Cargo.toml`

```toml
[package]
name = "dnsless-ui"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "dnsless-ui"
path = "src/main.rs"

[dependencies]
dnsless-common = { path = "../common" }
dnsless-server = { path = "../server" }
dnsless-client = { path = "../client" }
egui = "0.29"
eframe = "0.29"
```

**Sem** `serde`, `log`, `env_logger` como deps diretas do `ui` (a menos
que o Implementador demonstre necessidade — ex.: `env_logger` para não
silenciar os logs das threads de fundo). **Decisão humana de setup**:
o Implementador **deve** adicionar `env_logger = "0.11"` e `log = "0.4"`
ao `ui` para que os binários de fundo continuem emitindo logs no stderr
da UI (a refatoração preserva `log::`, então faz sentido inicializar o
logger no `main.rs` do ui). O `main.rs` do ui chama
`env_logger::Builder::from_env(...).init()` igual ao `server/main.rs`.

### `ui/src/event.rs`

```rust
//! Pure transformations from network events into UI log entries.

use dnsless_client::ClientEvent;
use dnsless_server::ServerEvent;

use crate::state::{LogEntry, LogKind};

/// Convert a server-side event into a human-readable log line.
/// Pure: no I/O, no egui. Unit-testable.
pub fn server_event_to_log_entry(ev: &ServerEvent) -> LogEntry {
    // ... match ev, construir (text, kind) ...
}

/// Convert a client-side event into a human-readable log line.
/// Pure: no I/O, no egui. Unit-testable.
pub fn client_event_to_log_entry(ev: &ClientEvent) -> LogEntry {
    // ... match ev, construir (text, kind) ...
}
```

### `ui/src/state.rs`

```rust
//! Observable UI state, mutated by events drained from the channels.

use std::net::SocketAddr;

use dnsless_client::ClientEvent;
use dnsless_server::ServerEvent;

/// Maximum lines kept in each panel's log history.
pub const LOG_CAPACITY: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogKind {
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    /// ISO-8601-ish local timestamp, e.g. "2026-08-23 23:45:01".
    /// (Formato não é normativo; usar chrono-free approach via SystemTime
    ///  + format manual, OU — decisão tomada — adicionar `chrono = "0.4"`
    ///  apenas com feature `clock` para timestamps legíveis. Ver Perguntas.)
    pub timestamp: String,
    pub text: String,
    pub kind: LogKind,
}

#[derive(Debug, Default)]
pub struct ServerState {
    pub bind_addr: Option<String>,
    pub interface: String,
    pub hostname: String,
    pub current_ip: Option<String>,
    pub connected_clients: Vec<String>, // "ip:port" — NUNCA hostname (lacuna nº 1)
    pub ip_history: Vec<(String, String)>, // (timestamp, "hostname -> ip")
    pub log: Vec<LogEntry>,
}

#[derive(Debug, Default)]
pub struct ClientState {
    pub server_addr: Option<String>,
    pub hosts_file: String,
    pub connected: bool,
    pub last_update: Option<(String, String)>, // (hostname, ip)
    pub update_history: Vec<(String, String, String)>, // (timestamp, hostname, ip)
    pub log: Vec<LogEntry>,
}

impl ServerState {
    pub fn new(cfg: &dnsless_server::config::ServerConfig) -> Self {
        // popula interface/hostname a partir da cfg; resto vazio
    }
    pub fn apply_server_event(&mut self, ev: ServerEvent) {
        // match -> muta campos + push LogEntry + truncate log
    }
}

impl ClientState {
    pub fn new(cfg: &dnsless_client::config::ClientConfig) -> Self {
        // popula server_addr/hosts_file a partir da cfg
    }
    pub fn apply_client_event(&mut self, ev: ClientEvent) {
        // match -> muta campos + push LogEntry + truncate log
    }
}
```

**Decisão de timestamp**: para não introduzir `chrono` (deps leve, coerente
com o projeto), usar `std::time::SystemTime` + formatar manualmente via
`humantime`? **Não** — decisão final: **adicionar `chrono = { version =
"0.4", default-features = false, features = ["clock"] }`** ao `ui`, porque
implementar formatação de tempo à mão a partir de `SystemTime` é
repetitivo e propenso a bug em datas. `chrono` com só `clock` é leve e
compila em Linux+Windows. Esta é uma decisão do Arquiteto, não suposição
do Implementador.

### `ui/src/net.rs`

```rust
//! Spawn the background network threads and hand back the event receivers.

use std::sync::mpsc;
use std::thread;

use dnsless_client::{ClientConfig, run_with_events as client_run_with_events};
use dnsless_server::{ServerConfig, run_with_events as server_run_with_events, ServerEvent};
use dnsless_client::ClientEvent;

/// Spawn one thread for the server loop and one for the client loop.
/// Returns the two receivers the UI will drain with try_recv.
pub fn spawn_network_threads(
    server_cfg: ServerConfig,
    client_cfg: ClientConfig,
) -> (mpsc::Receiver<ServerEvent>, mpsc::Receiver<ClientEvent>) {
    let (server_tx, server_rx) = mpsc::channel();
    let (client_tx, client_rx) = mpsc::channel();

    thread::Builder::new()
        .name("dnsless-server".into())
        .spawn(move || server_run_with_events(server_cfg, Some(server_tx)))
        .expect("failed to spawn server thread");

    thread::Builder::new()
        .name("dnsless-client".into())
        .spawn(move || client_run_with_events(client_cfg, Some(client_tx)))
        .expect("failed to spawn client thread");

    (server_rx, client_rx)
}
```

**Observação**: os `.expect(...)` em `thread::Builder::spawn` são
aceitáveis em caminho de produção (falha de spawn de thread é irrecuperável
e não há como degradar graciosamente). O Implementador pode manter
`.expect` AQUI apenas (rede de spawn é irrecuperável). Em todo o resto do
caminho de produção, **proibido `unwrap`/`expect`**.

### `ui/src/views/mod.rs`

```rust
//! egui rendering for each panel. Pure reads of state; never mutate.

use egui::{Context, Ui};

use crate::state::{ClientState, ServerState};

pub fn server_panel(ctx: &Context, ui: &mut Ui, state: &ServerState) {
    // ...
}
pub fn client_panel(ctx: &Context, ui: &mut Ui, state: &ClientState) {
    // ...
}
```

### `ui/src/app.rs`

```rust
//! The eframe::App that owns UI state and drains the event channels.

use std::sync::mpsc::{Receiver, TryRecvError};

use eframe::App;
use dnsless_client::ClientEvent;
use dnsless_server::ServerEvent;

use crate::state::{ClientState, ServerState};
use crate::views;

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
        Self { server_state, client_state, server_rx, client_rx }
    }

    fn drain(&mut self) {
        loop {
            match self.server_rx.try_recv() {
                Ok(ev) => self.server_state.apply_server_event(ev),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break,
            }
        }
        loop {
            match self.client_rx.try_recv() {
                Ok(ev) => self.client_state.apply_client_event(ev),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break,
            }
        }
    }
}

impl App for DnslessUiApp {
    /// Default dark theme (coerente com homelab; sem customização).
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain();

        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.heading("dnsless-homelab UI");
        });

        egui::SidePanel::left("server_panel")
            .resizable(true)
            .default_width(ctx.screen_rect().width() / 2.0)
            .show(ctx, |ui| {
                views::server_panel(ctx, ui, &self.server_state);
            });

        egui::SidePanel::right("client_panel")
            .resizable(true)
            .default_width(ctx.screen_rect().width() / 2.0)
            .show(ctx, |ui| {
                views::client_panel(ctx, ui, &self.client_state);
            });

        // se drenamos qualquer coisa, garantir repintura contínua
        ctx.request_repaint();
    }
}

impl Drop for DnslessUiApp {
    /// As threads de fundo rodam loops infinitos bloqueantes; não há
    /// maneira portable de cancelá-las. Quando a janela fecha, o processo
    /// do eframe termina e as threads órfãs morrem com ele. Os Senders
    /// caem em escopo aqui, então o próximo tx.send na thread retorna
    /// Err (ignorado, conforme spec).
    fn drop(&mut self) {}
}
```

### `ui/src/main.rs`

```rust
//! Thin binary entry point for dnsless-ui.

use std::env;

use dnsless_client::config::ClientConfig;
use dnsless_server::config::ServerConfig;
use eframe::egui;

use dnsless_ui::app::DnslessUiApp;
use dnsless_ui::net::spawn_network_threads;
use dnsless_ui::state::{ClientState, ServerState};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let server_cfg_path = arg_value("--server-config").unwrap_or_else(|| "server.toml".into());
    let client_cfg_path = arg_value("--client-config").unwrap_or_else(|| "client.toml".into());

    let server_cfg = ServerConfig::from_file(&server_cfg_path).unwrap_or_else(|e| {
        eprintln!("Warning: server config: {e}. Using default.");
        ServerConfig::default()
    });
    let client_cfg = ClientConfig::from_file(&client_cfg_path).unwrap_or_else(|e| {
        eprintln!("Warning: client config: {e}. Using default.");
        ClientConfig::default()
    });

    let (server_rx, client_rx) = spawn_network_threads(server_cfg.clone(), client_cfg.clone());
    let server_state = ServerState::new(&server_cfg);
    let client_state = ClientState::new(&client_cfg);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1024.0, 700.0])
            .with_min_inner_size([640.0, 400.0]),
        ..Default::default()
    };

    eframe::run_native(
        "dnsless-homelab UI",
        options,
        Box::new(move |cc| {
            // fundo escuro padrão do egui; nada a configurar.
            let _ = cc.egui_ctx;
            Ok(Box::new(DnslessUiApp::new(server_rx, client_rx, server_state, client_state)))
        }),
    )?;
    Ok(())
}

/// Read the value following a --flag from argv, if present.
fn arg_value(flag: &str) -> Option<String> {
    let mut args = env::args();
    while let Some(a) = args.next() {
        if a == flag {
            return args.next();
        }
    }
    None
}
```

**Atenção do Implementador**: a assinatura exata de `eframe::run_native`
em **0.29** é:

```rust
pub fn run_native(
    app_name: &str,
    native_options: NativeOptions,
    app_creator: AppCreator<'_>,
) -> Result<(), eframe::Error>
```

onde `AppCreator` é `Box<dyn for<'a> FnOnce(&'a CreationContext<'a>) -> Result<Box<dyn App>, Box<dyn Error + Send + Sync>>>`. Ou seja, **a closure retorna `Result<Box<dyn App>, Box<dyn Error...>>>`** (em 0.29, o `AppCreator` passou a retornar `Result`). O Implementador deve **confirmar** essa assinatura consultando `cargo doc -p eframe` ou `docs.rs/eframe/0.29` antes de finalizar. Se a toolchain instalada reclamar, ajustar o formato da closure — **não** remover o `?` de `run_native(...)?` que propaga `eframe::Error`. Reportar divergência na seção "Suposições".

### `ui/src/lib.rs`

```rust
//! dnsless-ui – egui dashboard for dnsless-homelab.

pub mod app;
pub mod event;
pub mod net;
pub mod state;
pub mod views;
```

---

## 7. Plano de testes

### Testáveis sem socket real (prioridade alta — implementar)

Em `ui/src/event.rs` (`#[cfg(test)] mod tests`):

- `server_event_to_log_entry` para cada variante de `ServerEvent`: assert
  que `text` contém substring esperada (ex.: `IpChanged` → contém o IP;
  `ClientConnected` → contém o `peer` formatado como `ip:port`); assert
  que `kind` é `Error` para `Error`, `Warn` para `PollUnchanged`? Não —
  `PollUnchanged` é `Info`. `Error` → `LogKind::Error`.
- `client_event_to_log_entry` análogo: `HostsFileError` → `kind == Error`
  e `text` contém substring da mensagem de erro; `HeartbeatReceived` →
  `Info` e texto curto; etc.

Em `ui/src/state.rs` (`#[cfg(test)] mod tests`):

- `apply_server_event(IpChanged{...})` atualiza `current_ip` e empurra
  entrada em `ip_history`; log cresce de 0 para 1.
- `apply_server_event(ClientConnected{peer})` adiciona `peer` a
  `connected_clients`; `ClientDisconnected` remove.
- Truncagem: popular `log` até `LOG_CAPACITY + 10` via eventos repetidos;
  assertar que `log.len() == LOG_CAPACITY` e que os itens retidos são os
  mais recentes (não os antigos). Importante: testa que `LOG_CAPACITY`
  funciona — bug silencioso em homelab de longa duração.
- `apply_client_event(HostsFileError{...})` não adiciona em
  `update_history` mas empurra `LogEntry` com `kind == Error`.
- `apply_client_event(HostsFileUpdated{...})` atualiza `last_update` e
  `update_history`.
- `apply_client_event(Connected{...})` seta `connected = true`;
  `ConnectionLost`/`ConnectionFailed` seta `connected = false`.

### Testes que exigem socket real (NÃO implementar — fora de escopo)

- `spawn_network_threads` de fato emitindo eventos (exigiria subir um
  `TcpListener` de teste e orquestrar — frágil, lento). Anotar como
  "integração manual" no README do ui se o Implementador julgar útil.
- Renderização visual dos painéis (exige egui real / screenshot). Fora.

### Critério: zero `thread::sleep`, zero socket real nos testes unitários.

O Implementador não deve usar `thread::sleep` ou abrir `TcpListener` em
testes. Se um teste depender de timing, refazer (é uma violação
explícita da filosofia de testes do projeto — ver `common/src/lib.rs`
e `client/src/hosts.rs` que só usam asserts puros).

---

## 8. Lista de tarefas numeradas para o Implementador

> Critério de aceite **objetivo e verificável** em cada uma.

1. **Adicionar `ui` ao workspace** — editar `Cargo.toml` raiz para
   incluir `"ui"` em `members`. **Aceite**: `cargo metadata` lista `ui`
   como membro; `cargo build -p dnsless-ui --no-default-features` não
   falha por "package not found" (pode falhar por código ausente até a
   tarefa 9).

2. **Refatorar `server/src/lib.rs`** — adicionar `pub enum ServerEvent`
   (derives `Debug, Clone`), `pub struct ClientConn` (privado ao módulo),
   trocar `Vec<TcpStream>` por `Vec<ClientConn>` em `accept_loop`/
   `broadcast`, adicionar `pub fn run_with_events(cfg, tx)` com o
   corpo refatorado mantendo TODOS os `log::` existentes + emissão de
   eventos, e reescrever `run(cfg)` como `run_with_events(cfg, None)`.
   **Aceite**: `cargo build -p dnsless-server` compila; `server/src/main.rs`
   **não é alterado**; `grep -n "info!\|warn!\|error!" server/src/lib.rs`
   retorna o mesmo número (ou maior) de ocorrências que o original; diff
   não remove nenhuma macro `log` existente.

3. **Refatorar `client/src/lib.rs`** — adicionar `pub enum ClientEvent`
   (derives `Debug, Clone`), `pub fn run_with_events(cfg, tx)` com corpo
   refatorado mantendo TODOS os `log::` existentes + emissão de eventos,
   reescrever `run(cfg)` como `run_with_events(cfg, None)`. **Aceite**:
   `cargo build -p dnsless-client` compila; `client/src/main.rs` não é
   alterado; mesmas contagens de `log::` que o original.

4. **Criar `ui/Cargo.toml`** — exatamente como na seção 6 (incluindo
   `env_logger`, `log`, `chrono = { version = "0.4", default-features =
   false, features = ["clock"] }`). **Aceite**: `cargo build -p dnsless-ui`
   resolve todas as deps a partir do `Cargo.lock` (pode baixar crates).

5. **Criar `ui/src/lib.rs`** exatamente como na seção 6. **Aceite**: 5
   linhas `pub mod` conforme listado.

6. **Criar `ui/src/state.rs`** — `LogKind`, `LogEntry`, `LOG_CAPACITY`,
   `ServerState`, `ClientState`, `apply_*_event`. Timestamps via
   `chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()`.
   **Aceite**: compila; os testes da tarefa 10 passam.

7. **Criar `ui/src/event.rs`** — `server_event_to_log_entry` e
   `client_event_to_log_entry`, match exaustivo sobre TODAS as variantes
   de `ServerEvent`/`ClientEvent`. **Aceite**: compila; sem `unreachable!`
   (match exaustivo não precisa); testes da tarefa 10 passam.

8. **Criar `ui/src/net.rs`** + **`ui/src/views/mod.rs`** + **`ui/src/app.rs`**
   + **`ui/src/main.rs`** conforme seção 6. **Aceite**: `cargo build -p
   dnsless-ui` compila sem warning (a não ser warnings conhecidos de egui
   em certas toolchains — reportar se houver). `cargo run -p dnsless-ui
   --server-config server/server.toml.example --client-config
   client/client.toml.example` abre a janela (verificação manual).

9. **Escrever testes unitários** em `event.rs` e `state.rs` conforme a
   seção 7. **Aceite**: `cargo test -p dnsless-ui` passa; nenhum teste
   usa `thread::sleep` ou `TcpListener`.

10. **Rodar `cargo fmt`, `cargo clippy --workspace`, `cargo test
    --workspace`** e reportar. **Aceite**: fmt sem diff; clippy sem
    warning novo em `server`/`client` (os `ui` podem ter avisos de egui
    não-ação — documentar); testes todos passam.

---

## 9. Perguntas em aberto que precisam de decisão humana

> O Implementador NÃO responde estas; ele as marca em "Perguntas para o
> Agente X" e segue com a melhor opção óbvia, sinalizando.

1. **`eframe::run_native` em 0.29 retorna `Result<(), eframe::Error>` e o
   `AppCreator` retorna `Result<Box<dyn App>, Box<dyn Error+Send+Sync>>`?**
   O Arquiteto tem 95% de confiança que sim (mudança introduzida em 0.28,
   mantida em 0.29), mas pede ao Implementador que **confirme via
   `cargo doc -p eframe --open` ou compilação** antes de finalizar. Se a
   toolchain reclamar, ajustar — não adivinhar.

2. **Tema**: usar o tema padrão do egui (claro/escuro conforme SO) ou
   forçar dark? O Arquiteto decide **forçar dark** (`ctx.set_visuals(
   egui::Visuals::dark())` no início de `update`) para consistência
   visual em homelab (normalmente em sala escura). Decidido; não é
   pergunta — apenas registrado para o Implementador aplicar.

Não há outras perguntas em aberto. O contexto fornecido (código-fonte
atual) foi **suficiente** para todas as decisões de arquitetura; nada
foi assumido por cima do que está nos arquivos lidos (`common/src/lib.rs`,
`server/src/{lib,config,ip_detector,main}.rs`, `client/src/{lib,config,
hosts,main}.rs`, `Cargo.toml` raiz, `README.md`, toolchain `rustc 1.98`).

---

## Referências consultadas (Arquiteto)

- `docs.rs/eframe/0.29.0` — versão alvo (https://docs.rs/crate/eframe/0.29.0)
- PR emilk/egui #7775 — propõe `App::logic`/`App::ui` em versão **posterior**
  a 0.29; não afeta este spec.
- Código-fonte do workspace (lido integralmente) — referência canônica.

**Fim do SPEC-UI.md**
