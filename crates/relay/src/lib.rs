//! Rendezvous relay: pairs a host's control connection with joiners by code, then splices bytes.
mod limits;
mod room;
mod splice;
#[cfg(test)]
mod tests;

pub use limits::Limits;

use alaz_relay_proto::{Code, FromRelay, MAX_RELAY_FRAME, RELAY_PROTOCOL, Refusal, ToRelay, read_msg, write_msg};
use room::{ConnSlot, Handoff, JoinSlot, Pending, Room, State};
use splice::splice;
use std::io::{ErrorKind, Read, Write};
use std::net::IpAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub trait Transport: Read + Write + Send + 'static {
    fn peer_ip(&self) -> IpAddr;
    fn shutdown(&self);
    fn set_read_timeout(&self, d: Option<Duration>);
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counters {
    pub rooms: u64,
    pub tunnels: u64,
    pub refusals: u64,
    pub lockouts: u64,
    pub abuse_cuts: u64,
}

pub struct Relay {
    pub(crate) limits: Limits,
    pub(crate) state: Mutex<room::State>,
    pub(crate) ips: room::IpTable,
    pub(crate) active_joins: AtomicUsize,
    pub(crate) tunnels: AtomicUsize,
    pub(crate) stats: room::Stats,
    pub(crate) attempts: Mutex<(Instant, u32)>,
}

impl Relay {
    pub fn new(limits: Limits) -> Arc<Relay> {
        Arc::new(Relay {
            limits,
            state: Mutex::default(),
            ips: room::IpTable::default(),
            active_joins: AtomicUsize::new(0),
            tunnels: AtomicUsize::new(0),
            stats: room::Stats::default(),
            attempts: Mutex::new((Instant::now(), 0)),
        })
    }

    /// Handles one connection on its own thread: host control (`Open`), host tunnel (`Accept`) or joiner (`Join`).
    pub fn serve<T: Transport>(self: &Arc<Self>, conn: T) {
        let ip = conn.peer_ip();
        let relay = self.clone();
        if !self.ips.try_add_conn(ip, self.limits.max_conns_per_ip) {
            thread::spawn(move || relay.refuse(&mut { conn }, Refusal::Busy));
            return;
        }
        let slot = ConnSlot { relay: self.clone(), ip };
        thread::spawn(move || relay.handle(conn, slot));
    }

    pub fn counters(&self) -> Counters {
        let rooms = self.state.lock().unwrap().rooms.len() as u64;
        Counters {
            rooms,
            tunnels: self.tunnels.load(Ordering::Relaxed) as u64,
            refusals: self.stats.refusals.load(Ordering::Relaxed),
            lockouts: self.stats.lockouts.load(Ordering::Relaxed),
            abuse_cuts: self.stats.abuse_cuts.load(Ordering::Relaxed),
        }
    }

    /// Counts the refusal, tells the peer why, closes. Never records the code or the peer.
    fn refuse<C: Read + Write>(&self, conn: &mut C, why: Refusal) {
        self.stats.refusals.fetch_add(1, Ordering::Relaxed);
        let _ = write_msg(conn, &FromRelay::Refused(why));
    }

    fn handle<T: Transport>(self: Arc<Self>, mut conn: T, slot: ConnSlot) {
        conn.set_read_timeout(Some(self.limits.control_idle));
        let first = read_msg::<ToRelay, _>(&mut conn);
        match first {
            Ok(ToRelay::Open { protocol }) => self.host_control(conn, protocol),
            Ok(ToRelay::Join { protocol, code }) => self.joiner(conn, protocol, code),
            Ok(ToRelay::Accept { id }) => self.accept(conn, slot, id),
            Ok(ToRelay::NewCode) | Err(_) => conn.shutdown(),
        }
    }

    fn attempt_allowed(&self) -> bool {
        let mut g = self.attempts.lock().unwrap();
        if g.0.elapsed() >= Duration::from_secs(1) {
            *g = (Instant::now(), 0);
        }
        g.1 += 1;
        g.1 <= self.limits.global_attempts_per_sec
    }

    /// A failed lookup: same delay and same reply whether the code never existed or expired.
    fn refuse_miss<C: Read + Write>(&self, conn: &mut C, ip: IpAddr) {
        if self.ips.record_fail(ip, &self.limits) {
            self.stats.lockouts.fetch_add(1, Ordering::Relaxed);
        }
        thread::sleep(self.limits.fail_delay);
        self.refuse(conn, Refusal::NoSuchCode);
    }

    fn fresh_code(state: &State) -> Code {
        loop {
            let c = Code::generate();
            if !state.rooms.contains_key(&c) {
                return c;
            }
        }
    }

    fn host_control<T: Transport>(self: &Arc<Self>, mut conn: T, protocol: u32) {
        if protocol != RELAY_PROTOCOL {
            return self.refuse(&mut conn, Refusal::BadProtocol);
        }
        let (events, inbox) = mpsc::channel();
        let room = Arc::new(Room { events, joiners: AtomicUsize::new(0) });
        let mut code = {
            let mut st = self.state.lock().unwrap();
            if st.rooms.len() >= self.limits.max_rooms {
                drop(st);
                return self.refuse(&mut conn, Refusal::Busy);
            }
            let c = Self::fresh_code(&st);
            st.rooms.insert(c, room.clone());
            c
        };
        self.control_loop(&mut conn, &room, &mut code, &inbox);
        let mut st = self.state.lock().unwrap();
        st.rooms.remove(&code);
        // joiners still waiting see the channel close and get a refusal
        st.pending.retain(|_, p| !Arc::ptr_eq(&p.room, &room));
        drop(st);
        conn.shutdown();
    }

    fn control_loop<T: Transport>(&self, conn: &mut T, room: &Arc<Room>, code: &mut Code, inbox: &mpsc::Receiver<u64>) {
        if write_msg(conn, &FromRelay::Opened { code: *code }).is_err() {
            return;
        }
        // frames are assembled by hand: a read timeout in the middle of read_exact would lose bytes
        conn.set_read_timeout(Some(Duration::from_millis(20)));
        let mut buf: Vec<u8> = Vec::new();
        let mut tmp = [0u8; 256];
        loop {
            while let Ok(id) = inbox.try_recv() {
                if write_msg(conn, &FromRelay::Incoming { id }).is_err() {
                    return;
                }
            }
            match conn.read(&mut tmp) {
                Ok(0) => return,
                Ok(n) => buf.extend_from_slice(&tmp[..n]),
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted) => {}
                Err(_) => return,
            }
            while let Some(len) = buf.get(..4).map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize) {
                if len > MAX_RELAY_FRAME {
                    return;
                }
                if buf.len() < 4 + len {
                    break;
                }
                let msg = read_msg::<ToRelay, _>(&mut &buf[..4 + len]);
                buf.drain(..4 + len);
                match msg {
                    Ok(ToRelay::NewCode) => {
                        let new = {
                            let mut st = self.state.lock().unwrap();
                            let new = Self::fresh_code(&st);
                            st.rooms.remove(code);
                            st.rooms.insert(new, room.clone());
                            new
                        };
                        *code = new;
                        if write_msg(conn, &FromRelay::Opened { code: new }).is_err() {
                            return;
                        }
                    }
                    _ => return,
                }
            }
        }
    }

    fn joiner<T: Transport>(self: &Arc<Self>, mut conn: T, protocol: u32, code: Code) {
        if protocol != RELAY_PROTOCOL {
            return self.refuse(&mut conn, Refusal::BadProtocol);
        }
        let ip = conn.peer_ip();
        if self.ips.is_locked(ip) {
            return self.refuse(&mut conn, Refusal::TooManyAttempts);
        }
        if !self.attempt_allowed() {
            return self.refuse(&mut conn, Refusal::Busy);
        }
        let id = {
            let mut raw = [0u8; 8];
            getrandom::fill(&mut raw).expect("OS randomness unavailable");
            u64::from_le_bytes(raw)
        };
        let (hand_tx, hand_rx) = mpsc::sync_channel(1);
        let (room, _slot) = {
            let mut st = self.state.lock().unwrap();
            let Some(room) = st.rooms.get(&code).cloned() else {
                drop(st);
                return self.refuse_miss(&mut conn, ip);
            };
            if room.joiners.load(Ordering::Relaxed) + 1 >= self.limits.max_players {
                drop(st);
                return self.refuse(&mut conn, Refusal::RoomFull);
            }
            if self.active_joins.load(Ordering::Relaxed) >= self.limits.max_tunnels {
                drop(st);
                return self.refuse(&mut conn, Refusal::Busy);
            }
            room.joiners.fetch_add(1, Ordering::Relaxed);
            self.active_joins.fetch_add(1, Ordering::Relaxed);
            let slot = JoinSlot { relay: self.clone(), room: room.clone() };
            st.pending.insert(id, Pending { room: room.clone(), handoff: hand_tx });
            (room, slot)
        };
        if room.events.send(id).is_err() {
            self.state.lock().unwrap().pending.remove(&id);
            return self.refuse_miss(&mut conn, ip);
        }
        let host = match hand_rx.recv_timeout(self.limits.pair_timeout) {
            Ok(h) => h,
            Err(RecvTimeoutError::Timeout) => {
                self.state.lock().unwrap().pending.remove(&id);
                match hand_rx.try_recv() {
                    Ok(h) => h,
                    Err(_) => return self.refuse(&mut conn, Refusal::Busy),
                }
            }
            Err(RecvTimeoutError::Disconnected) => return self.refuse(&mut conn, Refusal::NoSuchCode),
        };
        let mut host_conn = host.conn;
        if write_msg(&mut host_conn, &FromRelay::Paired).is_err() || write_msg(&mut conn, &FromRelay::Paired).is_err() {
            host_conn.shutdown();
            conn.shutdown();
            return;
        }
        self.tunnels.fetch_add(1, Ordering::Relaxed);
        if splice(Box::new(conn), host_conn, &self.limits) {
            self.stats.abuse_cuts.fetch_add(1, Ordering::Relaxed);
        }
        self.tunnels.fetch_sub(1, Ordering::Relaxed);
        // host._slot drops here with the host connection's per-IP slot
    }

    fn accept<T: Transport>(self: &Arc<Self>, mut conn: T, slot: ConnSlot, id: u64) {
        let ip = slot.ip;
        if self.ips.is_locked(ip) {
            return self.refuse(&mut conn, Refusal::TooManyAttempts);
        }
        let pending = self.state.lock().unwrap().pending.remove(&id);
        let Some(p) = pending else {
            return self.refuse_miss(&mut conn, ip);
        };
        if let Err(mpsc::SendError(mut h)) = p.handoff.send(Handoff { conn: Box::new(conn), _slot: slot }) {
            self.refuse(&mut h.conn, Refusal::NoSuchCode);
        }
    }
}
