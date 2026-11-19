use crate::Transport;
use crate::limits::Limits;
use alaz_relay_proto::Code;
use std::collections::{HashMap, VecDeque};
use std::net::IpAddr;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{Sender, SyncSender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// What a host's tunnel connection hands to the waiting joiner thread.
pub struct Handoff {
    pub conn: Box<dyn Transport>,
    pub _slot: ConnSlot,
}

pub struct Room {
    pub events: Sender<u64>,
    /// Joiners paired or waiting; the host is not counted.
    pub joiners: AtomicUsize,
}

pub struct Pending {
    pub room: Arc<Room>,
    pub handoff: SyncSender<Handoff>,
}

#[derive(Default)]
pub struct State {
    pub rooms: HashMap<Code, Arc<Room>>,
    pub pending: HashMap<u64, Pending>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Conn = 0,
    Room = 1,
    Tunnel = 2,
}

#[derive(Default)]
struct IpState {
    counts: [usize; 3],
    fails: VecDeque<Instant>,
    locked_until: Option<Instant>,
}

impl IpState {
    fn idle(&self) -> bool {
        self.counts == [0; 3] && self.fails.is_empty() && self.locked_until.is_none()
    }
}

#[derive(Default)]
pub struct Stats {
    pub refusals: AtomicU64,
    pub lockouts: AtomicU64,
    pub abuse_cuts: AtomicU64,
}

/// A /48 bucket tolerates this many times the per-/64 numbers for connections and failed guesses:
/// one site may hold several /64 networks, but a whole /48 must not multiply an attacker's budget by 65,536.
const WIDE: usize = 4;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Key(u8, u128);

/// The buckets an address counts against: itself for IPv4, its /64 and its /48 for IPv6. The stricter one wins.
fn buckets(ip: IpAddr) -> ([Key; 2], usize) {
    let v4 = |a: std::net::Ipv4Addr| ([Key(4, u32::from(a) as u128); 2], 1);
    match ip {
        IpAddr::V4(a) => v4(a),
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(a) => v4(a),
            None => {
                let b = u128::from(v6);
                (
                    [Key(64, b & (!0u128 << 64)), Key(48, b & (!0u128 << 80))],
                    2,
                )
            }
        },
    }
}

struct Inner {
    map: HashMap<Key, IpState>,
    last_sweep: Instant,
    #[cfg(test)]
    sweeps: usize,
}

/// Per-IP bookkeeping: connection, room and tunnel counts, failure windows, lockouts.
pub struct IpTable {
    window: Duration,
    sweep_every: Duration,
    inner: Mutex<Inner>,
}

impl IpTable {
    pub fn new(window: Duration) -> IpTable {
        IpTable {
            window,
            sweep_every: window.min(Duration::from_secs(1)),
            inner: Mutex::new(Inner {
                map: HashMap::new(),
                last_sweep: Instant::now(),
                #[cfg(test)]
                sweeps: 0,
            }),
        }
    }

    /// Drops entries that hold nothing live. Runs at most once per `sweep_every`, so a flood of
    /// connections costs O(1) per call on average instead of a full scan under the lock.
    fn sweep(&self, g: &mut Inner) {
        let now = Instant::now();
        if now.duration_since(g.last_sweep) < self.sweep_every {
            return;
        }
        g.last_sweep = now;
        #[cfg(test)]
        {
            g.sweeps += 1;
        }
        let window = self.window;
        g.map.retain(|_, s| {
            s.counts != [0; 3]
                || s.locked_until.is_some_and(|t| now < t)
                || s.fails
                    .back()
                    .is_some_and(|t| now.duration_since(*t) <= window)
        });
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().map.len()
    }

    #[cfg(test)]
    pub fn sweeps(&self) -> usize {
        self.inner.lock().unwrap().sweeps
    }

    #[cfg(test)]
    pub fn conns(&self, ip: IpAddr) -> usize {
        self.count(ip, Kind::Conn)
    }

    #[cfg(test)]
    pub fn count(&self, ip: IpAddr, kind: Kind) -> usize {
        let (keys, _) = buckets(ip);
        self.inner
            .lock()
            .unwrap()
            .map
            .get(&keys[0])
            .map_or(0, |s| s.counts[kind as usize])
    }

    /// Takes one unit of `kind` against every bucket of `ip`; false (nothing taken) when any is full.
    pub fn acquire(&self, ip: IpAddr, kind: Kind, max: usize) -> bool {
        let (keys, n) = buckets(ip);
        let mut g = self.inner.lock().unwrap();
        self.sweep(&mut g);
        for (i, k) in keys[..n].iter().enumerate() {
            let cap = if i == 0 || kind != Kind::Conn {
                max
            } else {
                max.saturating_mul(WIDE)
            };
            if g.map.get(k).is_some_and(|s| s.counts[kind as usize] >= cap) {
                return false;
            }
        }
        for k in &keys[..n] {
            g.map.entry(*k).or_default().counts[kind as usize] += 1;
        }
        true
    }

    pub fn release(&self, ip: IpAddr, kind: Kind) {
        let (keys, n) = buckets(ip);
        let mut g = self.inner.lock().unwrap();
        for k in &keys[..n] {
            if let Some(s) = g.map.get_mut(k) {
                s.counts[kind as usize] = s.counts[kind as usize].saturating_sub(1);
                if s.idle() {
                    g.map.remove(k);
                }
            }
        }
    }

    pub fn try_add_conn(&self, ip: IpAddr, max: usize) -> bool {
        self.acquire(ip, Kind::Conn, max)
    }

    #[cfg(test)]
    pub fn drop_conn(&self, ip: IpAddr) {
        self.release(ip, Kind::Conn);
    }

    pub fn is_locked(&self, ip: IpAddr) -> bool {
        let (keys, n) = buckets(ip);
        let mut g = self.inner.lock().unwrap();
        let now = Instant::now();
        let mut locked = false;
        for k in &keys[..n] {
            let Some(s) = g.map.get_mut(k) else { continue };
            match s.locked_until {
                Some(t) if now < t => locked = true,
                Some(_) => {
                    s.locked_until = None;
                    s.fails.clear();
                }
                None => {}
            }
        }
        locked
    }

    /// True when this address failed a lookup recently, in any of its buckets.
    pub fn has_recent_fails(&self, ip: IpAddr) -> bool {
        let (keys, n) = buckets(ip);
        let g = self.inner.lock().unwrap();
        let now = Instant::now();
        keys[..n].iter().any(|k| {
            g.map.get(k).is_some_and(|s| {
                s.fails
                    .back()
                    .is_some_and(|t| now.duration_since(*t) <= self.window)
            })
        })
    }

    /// Records a failed attempt in every bucket; true when this one starts a lockout.
    pub fn record_fail(&self, ip: IpAddr, lim: &Limits) -> bool {
        let (keys, n) = buckets(ip);
        let now = Instant::now();
        let mut g = self.inner.lock().unwrap();
        self.sweep(&mut g);
        let mut started = false;
        for (i, k) in keys[..n].iter().enumerate() {
            let max = lim.max_fails_per_window as usize * if i == 0 { 1 } else { WIDE };
            let s = g.map.entry(*k).or_default();
            while s
                .fails
                .front()
                .is_some_and(|t| now.duration_since(*t) > lim.fail_window)
            {
                s.fails.pop_front();
            }
            s.fails.push_back(now);
            if s.fails.len() > max && s.locked_until.is_none() {
                s.locked_until = Some(now + lim.lockout);
                started = true;
            }
        }
        started
    }
}

/// Frees one connection slot (per-IP and global) when dropped.
pub struct ConnSlot {
    pub relay: Arc<crate::Relay>,
    pub ip: IpAddr,
}

impl Drop for ConnSlot {
    fn drop(&mut self) {
        self.relay.ips.release(self.ip, Kind::Conn);
        self.relay.total_conns.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Frees one joiner place (room and global) when dropped.
pub struct JoinSlot {
    pub relay: Arc<crate::Relay>,
    pub room: Arc<Room>,
}

impl Drop for JoinSlot {
    fn drop(&mut self) {
        self.room.joiners.fetch_sub(1, Ordering::Relaxed);
        self.relay.active_joins.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Frees one per-IP room or tunnel unit when dropped.
pub struct IpCount {
    relay: Arc<crate::Relay>,
    ip: IpAddr,
    kind: Kind,
}

impl IpCount {
    pub fn take(relay: &Arc<crate::Relay>, ip: IpAddr, kind: Kind, max: usize) -> Option<IpCount> {
        relay.ips.acquire(ip, kind, max).then(|| IpCount {
            relay: relay.clone(),
            ip,
            kind,
        })
    }
}

impl Drop for IpCount {
    fn drop(&mut self) {
        self.relay.ips.release(self.ip, self.kind);
    }
}
