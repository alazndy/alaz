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

#[derive(Default)]
struct IpState {
    conns: usize,
    fails: VecDeque<Instant>,
    locked_until: Option<Instant>,
}

#[derive(Default)]
pub struct Stats {
    pub refusals: AtomicU64,
    pub lockouts: AtomicU64,
    pub abuse_cuts: AtomicU64,
}

/// IPv6 clients get whole /64 networks to themselves, so rotating addresses inside one does not dodge limits.
fn key(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V4(_) => ip,
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => IpAddr::V4(v4),
            None => IpAddr::V6((u128::from(v6) & (!0u128 << 64)).into()),
        },
    }
}

/// Per-IP bookkeeping: connection counts, failure windows, lockouts.
pub struct IpTable {
    window: Duration,
    inner: Mutex<(HashMap<IpAddr, IpState>, Instant)>,
}

impl IpTable {
    pub fn new(window: Duration) -> IpTable {
        IpTable {
            window,
            inner: Mutex::new((HashMap::new(), Instant::now())),
        }
    }

    /// Drops entries that hold nothing live; runs at most once per window (or when the table is large).
    fn sweep(&self, g: &mut (HashMap<IpAddr, IpState>, Instant)) {
        let now = Instant::now();
        if g.0.len() <= 256 && now.duration_since(g.1) < self.window {
            return;
        }
        g.1 = now;
        let window = self.window;
        g.0.retain(|_, s| {
            s.conns > 0
                || s.locked_until.is_some_and(|t| now < t)
                || s.fails
                    .back()
                    .is_some_and(|t| now.duration_since(*t) <= window)
        });
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().0.len()
    }

    #[cfg(test)]
    pub fn conns(&self, ip: IpAddr) -> usize {
        self.inner
            .lock()
            .unwrap()
            .0
            .get(&key(ip))
            .map_or(0, |s| s.conns)
    }

    pub fn try_add_conn(&self, ip: IpAddr, max: usize) -> bool {
        let mut g = self.inner.lock().unwrap();
        self.sweep(&mut g);
        let s = g.0.entry(key(ip)).or_default();
        if s.conns >= max {
            return false;
        }
        s.conns += 1;
        true
    }

    pub fn drop_conn(&self, ip: IpAddr) {
        let mut g = self.inner.lock().unwrap();
        if let Some(s) = g.0.get_mut(&key(ip)) {
            s.conns = s.conns.saturating_sub(1);
            if s.conns == 0 && s.fails.is_empty() && s.locked_until.is_none() {
                g.0.remove(&key(ip));
            }
        }
        self.sweep(&mut g);
    }

    pub fn is_locked(&self, ip: IpAddr) -> bool {
        let mut g = self.inner.lock().unwrap();
        let Some(s) = g.0.get_mut(&key(ip)) else {
            return false;
        };
        match s.locked_until {
            Some(t) if Instant::now() < t => true,
            Some(_) => {
                s.locked_until = None;
                s.fails.clear();
                false
            }
            None => false,
        }
    }

    /// Records a failed attempt; true when this one starts a lockout.
    pub fn record_fail(&self, ip: IpAddr, lim: &Limits) -> bool {
        let now = Instant::now();
        let mut g = self.inner.lock().unwrap();
        let s = g.0.entry(key(ip)).or_default();
        while s
            .fails
            .front()
            .is_some_and(|t| now.duration_since(*t) > lim.fail_window)
        {
            s.fails.pop_front();
        }
        s.fails.push_back(now);
        if s.fails.len() as u32 > lim.max_fails_per_window && s.locked_until.is_none() {
            s.locked_until = Some(now + lim.lockout);
            return true;
        }
        false
    }
}

/// Frees one per-IP connection slot when dropped.
pub struct ConnSlot {
    pub relay: Arc<crate::Relay>,
    pub ip: IpAddr,
}

impl Drop for ConnSlot {
    fn drop(&mut self) {
        self.relay.ips.drop_conn(self.ip);
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
