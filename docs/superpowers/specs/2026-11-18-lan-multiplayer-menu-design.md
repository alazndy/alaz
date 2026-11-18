# LAN multiplayer from the menu: design

Date: 2026-11-18. Size: small change (short design).

## Intent
The user wants to play co-op with people on the same local network. Today this works only from the command line
(`--host`, `--join ADDR`). Joining blocks the window for up to 8 seconds and exits the process on failure.
Goal: start and join a game from the start menu, and find hosts on the LAN without typing an IP.

## Goals
- A "Çok oyunculu" screen in the menu (same pattern as the "Tuşlar" and "Grafik" screens).
- Host: open the selected slot's world as a server. If the slot is empty, create a new world with the menu's
  current new-world settings. Port field, default `alaz_net::DEFAULT_PORT` (7777).
- Join: a list of hosts found on the LAN, plus a manual address field. Click to join.
- Connect on a background thread. Show "Bağlanıyor…" and show errors in the menu. Never exit the process.
- The host sees its LAN IP, port, and player count in the pause menu.

## Non-goals
- Internet play (NAT traversal, relay server).
- Competitive play, anti-cheat, separate tribes. The trust model stays "friends on one team".
- Changes to the lockstep protocol (`PROTOCOL` stays 1).

## Approaches
- A. Manual IP only. Least work. The joiner must ask for and type the IP.
- **B. Manual IP plus LAN discovery (picked by the user).** Small extra work, no new dependency.
  If a router or firewall blocks broadcast, manual IP still works.

## Decided
- Discovery lives in `crates/net` (new module), std only.
- Discovery is query/reply, so only the host binds a fixed port:
  - The joiner's screen sends a small UDP broadcast query to `255.255.255.255:7778` once per second, from an
    ephemeral port.
  - The host listens on UDP `7778` (non-blocking, polled with the host). It replies by unicast with: magic,
    protocol version, TCP port, player count, a short world label (slot and day).
  - The joiner lists replies seen in the last 3 seconds; it drops replies with another magic or protocol.
  - Reason: two game instances on one machine cannot both bind the same port without `SO_REUSEADDR`,
    which std does not expose. Only one host per machine binds 7778.
- If the discovery port cannot be bound, hosting still works; the screen says discovery is off.
- LAN IP shown to the host: a UDP socket "connected" to a private address (no packet is sent), then `local_addr()`.
- Background connect: `std::thread` + `mpsc` channel around the existing `Client::connect`.

## Left to the builder (my calls, can be changed)
- Exact layout of the screen. It follows the existing menu style (`card`, `section`, `button`).
- A joined client's pause menu hides the slot "Devam" and "Yeni dünya" actions (a client never writes saves).
- Tests: a discovery round trip on localhost in `crates/net`, and an egui_kittest test that the screen opens
  and the Join button reports an action.
- README "Çok oyunculu" section and `memory.md` get updated.
