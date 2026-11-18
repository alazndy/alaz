# LAN Multiplayer Menu Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Host and join a LAN co-op game from the start menu, with automatic discovery of hosts on the LAN.

**Architecture:** A new std-only `discovery` module in `crates/net` does UDP query/reply. `Host` answers queries
inside its existing `poll`. A new "Çok oyunculu" menu screen emits `MenuAction::Host` / `MenuAction::Join`.
`App` connects on a background thread and swaps the world in when the connection is ready.

**Tech Stack:** Rust 2024 workspace, egui 0.37 + egui_kittest, postcard + serde, std::net.

**Spec:** `docs/superpowers/specs/2026-11-18-lan-multiplayer-menu-design.md`

## Global Constraints
- No new crate dependencies. Discovery uses `std::net::UdpSocket` and the existing `postcard`/`serde`.
- `PROTOCOL` stays `1`. TCP default port stays `alaz_net::DEFAULT_PORT` (7777). Discovery port is `7778`.
- User-visible text is Turkish. Code, comments, commit messages are English, short.
- Never exit the process on a menu-started network error. Show it in the menu.
- Agents do not commit. The orchestrator reviews and commits each task.
- Run `cargo test -p <crate>` and `cargo clippy -p <crate> --all-targets` for every touched crate before reporting.

## Review Focus
1. Join to an address with nothing listening: menu shows an error within 8 s; window never freezes. (Task 3 test via `connect_in_background`.)
2. Garbage or foreign UDP packets on 7778: ignored, no panic. (Task 1 test `discovery_ignores_garbage`.)
3. Discovery port already in use (second host on one machine): hosting still works, a hint says discovery is off. (Task 1 test `enable_discovery_fails_cleanly_when_port_taken`.)
4. Host port already in use: menu shows the error, stays on the multiplayer screen, no world is replaced. (Task 3, manual check + code review.)
5. A host that stops answering disappears from the list after 3 s. (Task 1 test `finder_forgets_stale_hosts`.)

---

### Task 1: LAN discovery in `crates/net` — model: **Sonnet**

**Files:**
- Create: `crates/net/src/discovery.rs`
- Modify: `crates/net/src/lib.rs` (add `pub mod discovery;`, a `beacon` field on `Host`, `Host::enable_discovery`, answering in `Host::poll`)

**Interfaces (Produces):**
```rust
pub const DISCOVERY_PORT: u16 = 7778;
#[derive(Clone, Debug, PartialEq)]
pub struct Found { pub addr: std::net::SocketAddr /* host IP + TCP port */, pub players: u32, pub label: String }
pub struct Beacon { /* non-blocking UdpSocket */ }
impl Beacon {
    pub fn bind(port: u16) -> io::Result<Beacon>;           // binds 0.0.0.0:port
    pub fn local_port(&self) -> u16;
    pub fn answer(&self, tcp_port: u16, players: u32, label: &str); // reply to every pending query, never blocks
}
pub struct Finder { /* non-blocking UdpSocket on 0.0.0.0:0, broadcast enabled */ }
impl Finder {
    pub fn new() -> io::Result<Finder>;                      // target 255.255.255.255:DISCOVERY_PORT
    pub fn with_target(target: SocketAddr) -> io::Result<Finder>; // for tests (127.0.0.1:port)
    pub fn poll(&mut self);                                  // send a query if >= 1 s since last; read replies
    pub fn found(&self) -> Vec<Found>;                       // seen in the last 3 s, sorted by addr
}
pub fn lan_ip() -> Option<std::net::IpAddr>; // UdpSocket "connect" to 10.255.255.255:9 (no packet sent), then local_addr; None if loopback/unspecified
// in lib.rs:
impl Host { pub fn enable_discovery(&mut self, port: u16, label: String) -> io::Result<()>; }
```
- Wire format: postcard. `Query { magic: [u8; 4], protocol: u32 }`, `Reply { magic, protocol, tcp_port: u16, players: u32, label: String }`, magic `*b"ALAZ"`. Drop packets that fail to decode, have another magic, or another `PROTOCOL`. Truncate `label` to 64 chars on send.
- `Found.addr` = the reply's source IP + `tcp_port`.
- `Host::poll` calls `beacon.answer(self.listener port, self.players() as u32, &label)` when a beacon is set.
- `found()` takes `Instant` internally; make the staleness window a `const STALE: Duration = Duration::from_secs(3)` and add a test-only `Finder::found_at(now: Instant)` so the stale test does not sleep 3 s.

- [ ] **Step 1: Write failing tests in `discovery.rs` `#[cfg(test)]`**
  - `finder_sees_a_host_on_localhost`: `Beacon::bind(0)`, `Finder::with_target(127.0.0.1:beacon.local_port())`; loop up to 2 s: `finder.poll(); beacon.answer(7777, 2, "Yuva 1 · Gün 3"); finder.poll()`. Assert `found() == [Found { addr: 127.0.0.1:7777, players: 2, label: "Yuva 1 · Gün 3" }]`.
  - `discovery_ignores_garbage`: send `b"hello"` and a `Reply` with magic `*b"XXXX"` to the finder's socket and to the beacon; `poll`/`answer` do not panic; `found()` is empty.
  - `finder_forgets_stale_hosts`: after a host is found, `found_at(now + 4 s)` is empty.
  - `enable_discovery_fails_cleanly_when_port_taken` (in lib.rs tests): bind a `UdpSocket` on `0.0.0.0:0`, call `host.enable_discovery(that_port, ..)`, assert `Err`; then `host.poll(&world)` still works.
- [ ] **Step 2:** `cargo test -p alaz-net` → new tests FAIL (not defined).
- [ ] **Step 3:** Implement the interfaces above.
- [ ] **Step 4:** `cargo test -p alaz-net` → all PASS (4 old + 4 new). `cargo clippy -p alaz-net --all-targets` → no warnings.
- [ ] **Step 5:** Report diff summary. Do not commit.

### Task 2: "Çok oyunculu" menu screen in `crates/game/src/menu.rs` — model: **Sonnet** (runs in parallel with Task 1)

**Files:** Modify `crates/game/src/menu.rs` only.

**Interfaces (Produces):**
```rust
pub struct NewWorld { pub slot: u8, pub seed: Option<u64>, pub size: i32, pub hardcore: bool, pub settings: alaz_sim::world::WorldSettings }
pub enum MenuAction {
    // existing variants unchanged
    Host { slot: u8, port: u16, new: Option<NewWorld> }, // new = None: load the slot's save
    Join(String),                                         // raw text: "192.168.1.5" or "192.168.1.5:7777"
}
// new pub fields on Menu:
pub connecting: bool,            // App sets; disables the Join buttons and shows "Bağlanıyor…"
pub mp_error: Option<String>,    // App sets; shown in red on the multiplayer screen
pub net_line: Option<String>,    // App sets while hosting; shown under "Duraklatıldı" in the paused card
pub found: Vec<alaz_net::discovery::Found>, // App fills each frame from its Finder (Task 3)
pub mp_screen: bool,             // App reads it to know when to run the Finder
```
- Add a private `fn new_world(&self) -> NewWorld` that the existing "Yeni dünya başlat" button also uses (keep `MenuAction::New` as is, built from it).
- Button "Çok oyunculu" in the bottom row next to "Tuşlar" / "Grafik", **only when `!self.paused`**. Sets `mp_screen = true`. `ALAZ_DEMO_MPSCREEN` env opens it at start (like `ALAZ_DEMO_KEYS`).
- Screen layout (one `card`, existing `section` / `button` / `Look` helpers):
  - `section("Sunucu kur")`: line "Yuva {slot}: {dolu → 'kayıtlı dünya açılır' | boş → 'yeni dünya kurulur (menüdeki ayarlarla)'}"; a "PORT" text field (default `"7777"`, digits only); button "Sunucu kur" (Primary) → `Host { slot, port: parsed or 7777, new: slot empty → Some(self.new_world()) else None }`.
  - `section("Ağdaki oyunlar")`: one row per `found` entry: "{label} · {players} oyuncu · {ip}" with a "Katıl" button → `Join(addr.to_string())`. Empty list: "Aranıyor… Aynı ağda sunucu yoksa adresi elle yaz."
  - `section("Adresle katıl")`: text field (hint "192.168.1.5"), button "Katıl" → `Join(text.trim())`, ignored when empty.
  - `mp_error` in red; when `connecting`: "Bağlanıyor…" and Join buttons do nothing.
  - "Geri" (Plain) → `mp_screen = false`, clears `mp_error`.

- [ ] **Step 1: Failing kittest tests** (follow `starting_a_new_world_from_the_menu_works_with_a_click`):
  - `multiplayer_screen_hosts_the_selected_slot`: click "Çok oyunculu", then "Sunucu kur"; assert `Some(MenuAction::Host { port: 7777, .. })`.
  - `multiplayer_screen_joins_a_typed_address`: open screen, type "10.0.0.7" into the address field, click the "Katıl" button of that section; assert `Some(MenuAction::Join(a)) if a == "10.0.0.7"`.
  - `paused_menu_has_no_multiplayer_button`: `Menu::new(true)`; `query_by_label_contains("Çok oyunculu")` is None.
- [ ] **Step 2:** `cargo test -p alaz menu::` → new tests FAIL.
- [ ] **Step 3:** Implement. `alaz_net` is already a dependency of `alaz`; if Task 1 is not merged yet, use a local `pub struct Found` placeholder with the exact same fields and note it in the report (the orchestrator swaps it).
- [ ] **Step 4:** `cargo test -p alaz menu::` PASS; `cargo clippy -p alaz --all-targets` no new warnings.
- [ ] **Step 5:** Report. Do not commit.

### Task 3: Wire it into `App` in `crates/game/src/main.rs` — model: **Opus** (after Tasks 1 and 2)

**Files:** Modify `crates/game/src/main.rs`.

**Interfaces (Consumes):** Task 1 (`Finder`, `Found`, `lan_ip`, `DISCOVERY_PORT`, `Host::enable_discovery`), Task 2 (`MenuAction::Host/Join`, `NewWorld`, Menu pub fields).

**Produces:**
```rust
type Pending = std::sync::mpsc::Receiver<Result<(Client, World), alaz_net::NetError>>;
fn connect_in_background(addr: String) -> Pending;   // thread: Client::connect(&addr, 8 s)
fn normalize_addr(s: &str) -> String;                 // no ':' → append ":{DEFAULT_PORT}" (reuse the --join logic; make --join call it)
// new App fields:
connecting: Option<(String, Pending)>,
finder: Option<alaz_net::discovery::Finder>,          // Some only while menu.mp_screen; created lazily; Err → None + mp_error "Ağda arama kapalı"
```
- Refactor `M::Continue` and `M::New` bodies into `fn start_slot(&mut self, n: u8) -> Result<(), String>` and `fn start_new(&mut self, nw: menu::NewWorld)`; behaviour unchanged.
- `M::Host { slot, port, new }`: first `Host::bind(("0.0.0.0", port))`; on Err set `menu.mp_error = "Sunucu açılamadı (port {port}): {e}"` and return (world untouched). Then `start_new` or `start_slot` (on Err: mp_error, drop host). Then `enable_discovery(DISCOVERY_PORT, "Yuva {slot} · Gün {day}")`; on Err `say("Ağda bulunma kapalı: {e}. Arkadaşların adresi elle yazabilir.")`. Set `self.net = Net::Host(h)`, `say("Sunucu açık: {lan_ip}:{port}")`.
- The CLI `--host` path also calls `enable_discovery` (same label, same failure hint).
- `M::Join(text)`: ignore if `connecting.is_some()`; else `connecting = Some((normalize_addr(&text), connect_in_background(..)))`, `menu.connecting = true`, clear `mp_error`.
- Each frame before the menu draws: `try_recv` on `connecting`. `Ok(Ok((client, world)))` → `replace_world(world)`, `me = client.agent`, `net = Net::Client(client)`, `no_save = true`, `menu = None`, `fx.enter_world`, `say("{addr} sunucusuna katıldın.")`. `Ok(Err(e))` → `menu.mp_error = "Katılınamadı ({addr}): {e}"`, `menu.connecting = false`. `Disconnected` → same error with "bağlantı kesildi".
- Each frame while `menu.mp_screen`: create/poll `finder`, copy `found()` into `menu.found`. When the screen closes or the menu closes: `finder = None`.
- While hosting and the menu is open: `menu.net_line = Some("Sunucu açık: {lan_ip or '?'}:{port} · {players} oyuncu")`.

- [ ] **Step 1: Failing unit tests** in main.rs `#[cfg(test)]` (or the existing test module):
  - `normalize_addr_adds_default_port`: `"10.0.0.7"` → `"10.0.0.7:7777"`; `"10.0.0.7:9000"` unchanged.
  - `background_connect_reports_refused_quickly`: bind+drop a `TcpListener` on `127.0.0.1:0` to get a free port; `connect_in_background(that)`; `recv_timeout(10 s)` gives `Ok(Err(_))`; measure that the call to `connect_in_background` itself returns in < 100 ms.
  - `background_connect_joins_a_host`: `Host::bind("127.0.0.1:0")` with a small world; spawn connect; loop `host.poll(&w); host.turn(&w, vec![])` until `try_recv` is `Ok(Ok(_))` (≤ 5 s).
- [ ] **Step 2:** `cargo test -p alaz` → new tests FAIL.
- [ ] **Step 3:** Implement as above.
- [ ] **Step 4:** `cargo test -p alaz` all PASS (record count); `cargo clippy -p alaz --all-targets` no new warnings; `cargo build --release -p alaz` OK.
- [ ] **Step 5: Manual LAN smoke test** (orchestrator, two instances on one machine): instance A menu → Çok oyunculu → Sunucu kur; instance B → Çok oyunculu → host appears in "Ağdaki oyunlar" → Katıl → both see each other move. Then B: join `127.0.0.1:1` → red error, no freeze.
- [ ] **Step 6:** Report. Do not commit.

### Task 4: Docs — model: **Haiku** (after Task 3)

**Files:** Modify `README.md` (section "Çok oyunculu", ~line 215), `crates/game/assets/FONTS.md` untouched, project `memory.md` if it exists (append a trail line `[2026-11-18] [Claude Kaira]: LAN multiplayer menu + UDP discovery (port 7778).`).

- [ ] **Step 1:** In README "Çok oyunculu": add a short "Menüden" part before the CLI block: Çok oyunculu → Sunucu kur / Ağdaki oyunlar / Adresle katıl; discovery uses UDP 7778, game TCP 7777; Windows firewall asks once on first host, allow it; if the list stays empty, type the host IP. Keep the existing CLI lines.
- [ ] **Step 2:** Report the diff. Do not commit.

### Task 5: Final check — model: **Haiku** (run and report only)

- [ ] `cargo test --workspace 2>&1 | grep -E "test result|FAILED|panicked"` and `cargo clippy --workspace --all-targets 2>&1 | grep -cE "^warning|^error"`. Report the numbers and any failing test names. Read-only.
