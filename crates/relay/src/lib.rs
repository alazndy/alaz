//! Rendezvous relay: pairs a host's control connection with joiners by code, then splices bytes.
//! The server binary is Linux-only (SIGHUP reload, systemd deployment); the CI for other platforms builds only `-p alaz`.
mod limits;
mod room;
mod splice;
#[cfg(test)]
mod tests;
pub mod tls;

pub use limits::Limits;

use alaz_relay_proto::{
    Code, FromRelay, MAX_RELAY_FRAME, RELAY_PROTOCOL, Refusal, ToRelay, read_msg, write_msg,
};
use room::{ConnSlot, Handoff, IpCount, JoinSlot, Kind, Pending, Room, State};
use splice::splice;
use std::io::{ErrorKind, Read, Write};
use std::net::IpAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// Starts a thread with the relay's small stack. The error is the caller's to handle: a refused
/// thread must drop the connection, never panic (a panic aborts the whole process in release builds).
pub(crate) fn spawn_small<F: FnOnce() + Send + 'static>(
    stack: usize,
    f: F,
) -> std::io::Result<thread::JoinHandle<()>> {
    thread::Builder::new().stack_size(stack).spawn(f)
}

pub trait Transport: Read + Write + Send + 'static {
    fn peer_ip(&self) -> IpAddr;
    fn shutdown(&self);
    fn set_read_timeout(&self, d: Option<Duration>);
    fn set_write_timeout(&self, d: Option<Duration>);
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
    /// (window start, attempts, attempts by IPs with recent failures)
    pub(crate) attempts: Mutex<(Instant, u32, u32)>,
    pub(crate) total_conns: AtomicUsize,
    #[cfg(test)]
    pub(crate) spawned: AtomicUsize,
}

impl Relay {
    pub fn new(limits: Limits) -> Arc<Relay> {
        Arc::new(Relay {
            ips: room::IpTable::new(limits.fail_window),
            limits,
            state: Mutex::default(),
            active_joins: AtomicUsize::new(0),
            tunnels: AtomicUsize::new(0),
            stats: room::Stats::default(),
            attempts: Mutex::new((Instant::now(), 0, 0)),
            total_conns: AtomicUsize::new(0),
            #[cfg(test)]
            spawned: AtomicUsize::new(0),
        })
    }

    /// Decides at accept time, before any TLS work and before any thread exists, whether this peer
    /// may hold a connection. None means: close the socket and say nothing.
    pub fn admit(self: &Arc<Self>, ip: IpAddr) -> Option<ConnSlot> {
        if self.total_conns.fetch_add(1, Ordering::Relaxed) >= self.limits.max_total_conns {
            self.total_conns.fetch_sub(1, Ordering::Relaxed);
            return None;
        }
        if self.ips.is_locked(ip) || !self.ips.try_add_conn(ip, self.limits.max_conns_per_ip) {
            self.total_conns.fetch_sub(1, Ordering::Relaxed);
            return None;
        }
        Some(ConnSlot {
            relay: self.clone(),
            ip,
        })
    }

    /// Admits and serves one connection (the accept loop calls `admit` itself, before the TLS wrapper exists).
    pub fn serve<T: Transport>(self: &Arc<Self>, conn: T) {
        match self.admit(conn.peer_ip()) {
            Some(slot) => self.serve_admitted(conn, slot),
            None => conn.shutdown(),
        }
    }

    /// Handles one admitted connection on its own thread: host control (`Open`), host tunnel (`Accept`)
    /// or joiner (`Join`). If the OS refuses the thread the connection is dropped.
    pub fn serve_admitted<T: Transport>(self: &Arc<Self>, conn: T, slot: ConnSlot) {
        let relay = self.clone();
        #[cfg(test)]
        self.spawned.fetch_add(1, Ordering::Relaxed);
        let _ = spawn_small(self.limits.stack_size, move || relay.handle(conn, slot));
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
        // a stuck peer must not park this thread inside a write
        conn.set_write_timeout(Some(self.limits.control_idle));
        let mut frames = Frames::default();
        let first = frames.read_until(&mut conn, Instant::now() + self.limits.first_frame_timeout);
        match first {
            Ok(ToRelay::Open { protocol }) => self.host_control(conn, protocol),
            Ok(ToRelay::Join { protocol, code }) => self.joiner(conn, protocol, code),
            Ok(ToRelay::Accept { id }) => self.accept(conn, slot, id),
            Ok(ToRelay::NewCode | ToRelay::Ping) | Err(_) => conn.shutdown(),
        }
    }

    /// One shared budget per second. Addresses with a recent failed guess may use only the part above
    /// the reserve, so a guesser who spends the budget still leaves room for everyone else.
    pub(crate) fn attempt_allowed(&self, has_recent_fails: bool) -> bool {
        let lim = &self.limits;
        let mut g = self.attempts.lock().unwrap();
        if g.0.elapsed() >= Duration::from_secs(1) {
            *g = (Instant::now(), 0, 0);
        }
        if has_recent_fails {
            if g.2
                >= lim
                    .global_attempts_per_sec
                    .saturating_sub(lim.reserved_attempts_per_sec)
            {
                return false;
            }
            g.2 += 1;
        }
        g.1 += 1;
        g.1 <= lim.global_attempts_per_sec
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
        let Some(_room_ip) = IpCount::take(
            self,
            conn.peer_ip(),
            Kind::Room,
            self.limits.max_rooms_per_ip,
        ) else {
            return self.refuse(&mut conn, Refusal::Busy);
        };
        let (events, inbox) = mpsc::channel();
        let room = Arc::new(Room {
            events,
            joiners: AtomicUsize::new(0),
        });
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

    fn control_loop<T: Transport>(
        &self,
        conn: &mut T,
        room: &Arc<Room>,
        code: &mut Code,
        inbox: &mpsc::Receiver<u64>,
    ) {
        if write_msg(conn, &FromRelay::Opened { code: *code }).is_err() {
            return;
        }
        let mut frames = Frames::default();
        let mut last_frame = Instant::now();
        loop {
            while let Ok(id) = inbox.try_recv() {
                if write_msg(conn, &FromRelay::Incoming { id }).is_err() {
                    return;
                }
            }
            if last_frame.elapsed() > self.limits.control_idle {
                return;
            }
            loop {
                match frames.next() {
                    Ok(None) => break,
                    Ok(Some(ToRelay::Ping)) => last_frame = Instant::now(),
                    Ok(Some(ToRelay::NewCode)) => {
                        last_frame = Instant::now();
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
            if frames.fill(conn).is_err() {
                return;
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
        if !self.attempt_allowed(self.ips.has_recent_fails(ip)) {
            return self.refuse(&mut conn, Refusal::Busy);
        }
        let mut raw = [0u8; 8];
        if getrandom::fill(&mut raw).is_err() {
            return self.refuse(&mut conn, Refusal::Busy);
        }
        let id = u64::from_le_bytes(raw);
        let Some(_tunnel_ip) =
            IpCount::take(self, ip, Kind::Tunnel, self.limits.max_tunnels_per_ip)
        else {
            return self.refuse(&mut conn, Refusal::Busy);
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
            let slot = JoinSlot {
                relay: self.clone(),
                room: room.clone(),
            };
            st.pending.insert(
                id,
                Pending {
                    room: room.clone(),
                    handoff: hand_tx,
                },
            );
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
            Err(RecvTimeoutError::Disconnected) => {
                return self.refuse(&mut conn, Refusal::NoSuchCode);
            }
        };
        let mut host_conn = host.conn;
        if write_msg(&mut host_conn, &FromRelay::Paired).is_err()
            || write_msg(&mut conn, &FromRelay::Paired).is_err()
        {
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
        // exempt from lockout and fail counting: the 64-bit random id authenticates the host, and a host
        // behind the same NAT as a mistyping joiner must not be locked out
        let pending = self.state.lock().unwrap().pending.remove(&id);
        let Some(p) = pending else {
            return self.refuse(&mut conn, Refusal::NoSuchCode);
        };
        if let Err(mpsc::SendError(mut h)) = p.handoff.send(Handoff {
            conn: Box::new(conn),
            _slot: slot,
        }) {
            self.refuse(&mut h.conn, Refusal::NoSuchCode);
        }
    }
}

/// Assembles frames by hand: a read timeout in the middle of `read_exact` would lose bytes.
#[derive(Default)]
struct Frames {
    buf: Vec<u8>,
}

impl Frames {
    /// Ok(None) when no whole frame is buffered yet; Err on an oversize or undecodable frame.
    fn next(&mut self) -> Result<Option<ToRelay>, ()> {
        let Some(len) = self
            .buf
            .get(..4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize)
        else {
            return Ok(None);
        };
        if len > MAX_RELAY_FRAME {
            return Err(());
        }
        if self.buf.len() < 4 + len {
            return Ok(None);
        }
        let msg = read_msg::<ToRelay, _>(&mut &self.buf[..4 + len]).map_err(|_| ());
        self.buf.drain(..4 + len);
        msg.map(Some)
    }

    /// One read of at most the connection's poll timeout; Err when the peer is gone.
    fn fill<T: Transport>(&mut self, conn: &mut T) -> Result<(), ()> {
        conn.set_read_timeout(Some(POLL));
        let mut tmp = [0u8; 256];
        match conn.read(&mut tmp) {
            Ok(0) => Err(()),
            Ok(n) => {
                self.buf.extend_from_slice(&tmp[..n]);
                Ok(())
            }
            Err(e)
                if matches!(
                    e.kind(),
                    ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted
                ) =>
            {
                Ok(())
            }
            Err(_) => Err(()),
        }
    }

    /// Reads one frame, giving up when `deadline` passes however slowly bytes trickle in.
    fn read_until<T: Transport>(&mut self, conn: &mut T, deadline: Instant) -> Result<ToRelay, ()> {
        loop {
            if let Some(m) = self.next()? {
                return Ok(m);
            }
            if Instant::now() >= deadline {
                return Err(());
            }
            self.fill(conn)?;
        }
    }
}

const POLL: Duration = Duration::from_millis(20);
