# Feedback In Game Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A tester presses `F8`, writes a report, and the game uploads it with a screenshot, a log, and the save.

**Architecture:** A new crate `alaz-feedback` holds everything that needs no window: report limits, text scrubbing, PNG, game log, crash file, outbox, upload. The renderer gains an in-memory frame capture. The game crate adds the form and connects the two.

**Tech Stack:** Rust, `png` 0.18.1, `ureq` 3.5.2 (rustls), `serde`/`serde_json`, `getrandom` 0.3 (all already in `Cargo.lock`), egui, egui_kittest.

**Spec:** `docs/superpowers/specs/2026-11-26-feedback-reports-design.md`, "Game side". The server rules are in `web/feedback/src/lib/report.ts`.

## Global Constraints

- Sizes in bytes: screenshot 2_500_000; log 300_000; save 1_000_000; fields 20_000; request 4_000_000. Text 1..4000 characters; name ≤40.
- Nothing is sent without a press of "Gönder".
- Address: `https://feedback.alazlab.com`, override `ALAZ_FEEDBACK_URL`; only `https`, or `http` to this machine.
- Tests never open a network connection to another machine and never touch the player's data.
- No secret in the game. No change to the simulation, the save format, or `PROTOCOL`.
- A report never writes to a save slot.
- Interface text is Turkish.

## Review Focus

- The form is open and the player types `w`, `m`, `Esc`: only text appears; nothing moves; `Esc` closes (Task 6).
- The game closes or the machine loses power while a report is written: no half report is ever sent (Task 3).
- The site is not live yet, or a wrong address answers 404: the report stays in the outbox (Task 3).
- A log holds the home path, the user name, an address, or a room code: none reaches the server (Task 1).
- The graphics surface cannot be copied: the form still opens, without a screenshot (Task 6).

---

### Task 1: Report, scrubbing, screenshot

**Files:**
- Create: `crates/feedback/Cargo.toml`, `src/lib.rs`, `src/report.rs`, `src/scrub.rs`, `src/shot.rs`
- Modify: `Cargo.toml` (workspace member)

**Interfaces:**
- `report`: constants `SCREENSHOT_MAX`, `LOG_MAX`, `SAVE_MAX`, `BODY_MAX`, `FIELDS_MAX`, `TEXT_MAX`, `NAME_MAX`; `enum Kind { Bug, Idea }`; `enum Mode { Solo, Host, Guest, Menu }`; `enum Part { Screenshot, Log, Save }` with `name()` (lower case); `struct Fields` with the server's field names; `struct Draft { kind, text, name, crash, game_version, os, gpu: Option<String>, window: Option<String>, zoom: Option<f32>, seed: Option<u64>, world_size: Option<u32>, day: Option<u32>, mode, screenshot, log, save: Option<Vec<u8>> }`; `struct Report { fields: Fields, files: Vec<(Part, Vec<u8>)> }`; `Draft::build(self, id: String) -> Result<Report, Problem>` where `enum Problem { Empty, TooLong }`; `new_id() -> String` (32 lower hex from the operating system's random source).
- `build`: removes control characters except newline and tab; trims text; cuts name, os (64), gpu (128), window (32) to length; keeps zoom only in 0.2..=4; a file over its size moves to `omitted`; seed becomes a decimal string.
- `scrub::scrub(text: &str, home: Option<&str>, user: Option<&str>) -> String`: home path (both slash directions) → `~`; user name of 3 or more characters → `<user>`; IPv4 and IPv6 addresses with or without port → `<addr>`; room codes (8 characters of `ABCDEFGHJKLMNPQRSTUVWXYZ23456789`, with or without one hyphen in the middle) → `<code>`.
- `shot::shrink(rgb: &[u8], width: u32, height: u32, max_width: u32) -> (Vec<u8>, u32, u32)`; `shot::png(rgb, width, height, max_width: u32, max_bytes: usize) -> Option<Vec<u8>>`: shrinks to `max_width`, then halves until the file fits; `None` below 160 wide or for a buffer of the wrong length.

- [ ] **Step 1: Write the failing tests** in each module: empty and 4001-character text refused, 4000 four-byte characters accepted and fields JSON ≤ `FIELDS_MAX`; JSON uses the keys `game_version`, `world_size`, `omitted` and omits absent options; over-size parts are listed in `omitted`; ids are 32 hex and differ; scrub cases for each rule, plus `12:03:04`, version `0.6.0`, a 32-hex report id, and ordinary Turkish words stay; `shrink` halves a 4x2 picture by averaging; `png` output decodes to the expected size, obeys `max_width` and `max_bytes` with noise input, and refuses a short buffer.
- [ ] **Step 2:** `cargo test -p alaz-feedback`: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** `cargo test -p alaz-feedback`: PASS. `cargo audit`: no new finding.
- [ ] **Step 5:** Commit `feat(feedback): report limits, scrubbing and screenshot encoding`.

### Task 2: Game log and crash file

**Files:**
- Create: `crates/feedback/src/log.rs`, `src/crash.rs`

**Interfaces:**
- `log::start(dir: &Path)` moves `game.log` to `game.old.log` and opens a new file; `log::line(msg: &str)` prints to stderr and appends `[+12.3s] msg` when started; the file never exceeds `log::CAP` = 1_000_000 (the older half is dropped). `log::tail(path, max: usize) -> String` (whole characters, no NUL, empty when missing). `log::compose(game: &str, play: &str) -> Vec<u8>`: `playlog` last 200 lines after the game log, ≤ `LOG_MAX`, the game log loses its start when too long.
- `crash::write(dir, slot: u8, text: &str)`, `crash::take(dir) -> Option<Crash { slot: u8, text: String }>` (returns each crash once: renames to `crash.last.txt`), `crash::install(dir: PathBuf, slot: fn() -> u8)` (panic hook that calls `write` and `log::line`, then the previous hook), `crash::forget_own(dir)` removes a crash file this run wrote.

- [ ] **Step 1: Failing tests:** rotation keeps exactly one old file; the cap holds after 2 MB of lines and the newest line survives; `tail` cuts on a character boundary; `compose` order, 200-line limit, size limit; `take` returns once; a missing file is `None`; `forget_own` leaves a crash from an earlier run alone. One test installs the hook, panics inside `std::panic::catch_unwind`, and finds the message and slot.
- [ ] **Step 2-4:** FAIL, implement, `cargo test -p alaz-feedback` PASS.
- [ ] **Step 5:** Commit `feat(feedback): game log and crash file`.

### Task 3: Outbox and upload

**Files:**
- Create: `crates/feedback/src/outbox.rs`, `src/upload.rs`

**Interfaces:**
- `outbox::WAITING_MAX` = 10; `enum Outcome { Sent, Later, Refused }`; `enum PutError { Full, Io(std::io::Error) }`; `put(dir, &Report) -> Result<(), PutError>` writes `.tmp-<id>/` then renames to `<id>/`; `waiting(dir) -> usize`; `flush(dir, send: &mut dyn FnMut(&Report) -> Outcome) -> Vec<(String, Outcome)>` oldest first, stops at `Later`, deletes on `Sent`, moves `Refused` and unreadable reports to `refused/` (newest 10 kept); only one flush runs at a time in a process.
- `upload::URL`; `upload::base(over: Option<&str>) -> Option<String>` (None for an address that is not allowed, never a fall back); `upload::body(&Report, boundary: &str) -> Vec<u8>`; `upload::send(base: &str, &Report) -> Outcome`: 200/201 → `Sent`; 400 or 413 → `Refused`; everything else, redirects and network errors → `Later`. User agent `alaz-feedback`. Timeout 60 s. No redirects.

- [ ] **Step 1: Failing tests:** put/flush round trip keeps fields and files; the 11th put is `Full` and nothing is lost; a leftover `.tmp-` folder is neither counted nor sent; `Later` keeps this and all newer; `Refused` moves aside and the next one is sent; a broken `report.json` moves aside; two threads flushing send each report once; `base` cases (`https://x.example`, trailing slash removed, `http://localhost:3000`, `http://127.0.0.1:1`, `http://[::1]:2` allowed; `http://example.com`, `ftp://x`, `https://a@b`, `http://localhost.evil.com` refused); `body` has the `report` field without a file name and each file part; `send` against a `TcpListener` on 127.0.0.1 for 201, 200, 400, 404, 413, 429, 503, a redirect, and a closed port.
- [ ] **Step 2-4:** FAIL, implement, PASS.
- [ ] **Step 5:** Commit `feat(feedback): outbox and upload`.

### Task 4: Frame capture

**Files:**
- Modify: `crates/render/src/renderer.rs`, `crates/render/src/renderer/frame.rs`

**Interfaces:**
- `pub struct Capture { width: u32, height: u32, rgb: Vec<u8> }`; `Renderer::request_capture()`, `take_capture() -> Option<Capture>`, `can_capture() -> bool`, `adapter_name() -> &str`. `COPY_SRC` is set whenever the surface supports it. `write_shot` uses the same read-back function. A pure `fn rgb_rows(data: &[u8], width, height, row: usize, bgra: bool) -> Vec<u8>`.

- [ ] **Step 1: Failing test** for `rgb_rows`: padded rows, both channel orders.
- [ ] **Step 2-4:** FAIL, implement, `cargo test -p alaz-render rgb_rows` PASS.
- [ ] **Step 5:** Commit `feat(render): capture a finished frame in memory`.

### Task 5: Key and form

**Files:**
- Modify: `crates/game/src/keys.rs`
- Create: `crates/game/src/report_ui.rs`

**Interfaces:**
- `Action::Report` (id `report`, name "Geri bildirim", default `F8`); an old `keys.cfg` without a `report` line that uses `F8` elsewhere leaves `Report` unbound.
- `report_ui::Form { kind, text, name, send_shot, send_log, send_save: bool, preview: Option<(Vec<u8>, u32, u32)>, has_save: bool, notice: Option<String> }`; `enum FormAct { Send, Cancel }`; `draw_form(ctx, &mut Form) -> Option<FormAct>`; `draw_crash_prompt(ctx) -> Option<bool>`; `draw_toast(ctx, text)`.
- Form labels: heading "Geri bildirim"; "Hata", "İstek"; text box named "Ne oldu?"; "İsmin (istersen)"; boxes "Ekran görüntüsü", "Loglar", "Kayıt dosyası" (the first and last disabled when absent); "Raporu geliştirici ve davetli testçiler görür."; counter "n / 4000"; buttons "Gönder" (disabled for empty or over-long text) and "Vazgeç". Crash prompt: "Oyun geçen sefer çöktü. Rapor gönderilsin mi?", "Gönder", "Hayır". The window fits 800x600 at interface zoom 1.5 (its middle scrolls).

- [ ] **Step 1: Failing tests:** key default, migration both ways; egui_kittest: send disabled when empty, enabled after typing, returns `Send`; "Vazgeç" returns `Cancel`; choosing "İstek" sets the kind; boxes disabled when there is no picture or save; the prompt returns true and false; at 800x600 zoom 1.5 "Gönder" and "Vazgeç" lie inside the window.
- [ ] **Step 2-4:** FAIL, implement, `cargo test -p alaz` PASS.
- [ ] **Step 5:** Commit `feat(game): feedback key and form`.

### Task 6: Connect the game

**Files:**
- Create: `crates/game/src/feedback.rs`
- Modify: `crates/game/src/main.rs`, `menu.rs`, `saveio.rs`, `crates/game/Cargo.toml`

**Interfaces:**
- `feedback::Feedback` held by `App`: `open_request()`, `is_open()`, form, crash prompt, background results, toast; the send function is a field so tests replace the network.
- `F8` works in play and in menus. The next finished frame is captured, then the form opens; after 5 frames without a picture it opens without one. A pause and start screen button "Geri bildirim" does the same.
- While the form or prompt is open: keys reach only the text box (`Esc` closes), clicks and wheel do not reach the world, a solo world does not step.
- Send: build the draft (save = `world.to_bytes()` when a world is in play; log = `compose` of the two logs, scrubbed), `outbox::put`, flush on a thread; toast "Gönderildi", "Sonra gönderilecek", or "Sunucu raporu kabul etmedi."; a full outbox keeps the form open with "Bekleyen 10 rapor var; yenisi eklenemiyor." The name is kept in `reports-name.txt`.
- Start: `log::start`, `crash::install` (replaces `install_crash_log`), a start line with version, system and adapter; a crash found shows the prompt; waiting reports are flushed. A crash report: text "Oyun geçen sefer çöktü." plus the first 20 lines of the crash text (scrubbed), log from `game.old.log` plus the crash text, the main save of the crash's slot, mode `menu`, crash true. A clean exit calls `crash::forget_own`.
- The game's `eprintln!` calls for load, save, join, host and map errors go through `log::line`; add lines for new world, load, save, join, disconnect.
- `ALAZ_DEMO_TAB=report` opens the form (screenshots).

- [ ] **Step 1: Failing tests** on a windowless `App` (as `map_tests` does): typing keys while open neither moves nor triggers actions, `Esc` closes; a solo world's tick does not advance while open; sending in play puts one report with log and save into the outbox and leaves the save slot files untouched; the menu sends mode `menu` without a save; a crash on disk yields the prompt, "Hayır" sends nothing, "Gönder" queues a report with `crash` true; a full outbox keeps the form and text; F8 in a menu opens the form.
- [ ] **Step 2-4:** FAIL, implement, `cargo test -p alaz` PASS.
- [ ] **Step 5:** Commit `feat(game): send reports from the game`.

### Task 7: Real run and documents

**Files:**
- Modify: `README.md`, `memory.md`, `web/feedback/README.md`

- [ ] **Step 1:** Start the site in local mode. Run an ignored test `sends_to_a_local_site` (reads `ALAZ_FEEDBACK_URL`) that builds a real report with a PNG, a log, and a world save and expects `Sent`, twice with the same id.
- [ ] **Step 2:** Run the release game with `ALAZ_DEMO_TAB=report` at 1280x720 and at zoom 1.5; look at both screenshots.
- [ ] **Step 3:** Documents: `F8`, the environment variables, what a report holds. `cargo test --workspace`; `cargo clippy --workspace --all-targets`: no new warning on changed lines; `git diff main --stat -- crates/sim` empty; `pnpm --dir web/feedback test`.
- [ ] **Step 4:** Commit and push `feat/feedback`. Do not merge.
