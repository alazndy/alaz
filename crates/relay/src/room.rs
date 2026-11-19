use crate::Transport;
use crate::limits::Limits;
use alaz_relay_proto::Code;
use std::collections::{HashMap, VecDeque};
use std::net::IpAddr;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{Sender, SyncSender};
use std::sync::{Arc, Mutex};
use std::time::Instant;

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

/// Per-IP bookkeeping: connection counts, failure windows, lockouts.
#[derive(Default)]
pub struct IpTable {
    map: Mutex<HashMap<IpAddr, IpState>>,
}

impl IpTable {
    pub fn try_add_conn(&self, ip: IpAddr, max: usize) -> bool {
        let mut m = self.map.lock().unwrap();
        let s = m.entry(ip).or_default();
        if s.conns >= max {
            return false;
        }
        s.conns += 1;
        true
    }

    pub fn drop_conn(&self, ip: IpAddr) {
        let mut m = self.map.lock().unwrap();
        if let Some(s) = m.get_mut(&ip) {
            s.conns = s.conns.saturating_sub(1);
            if s.conns == 0 && s.fails.is_empty() && s.locked_until.is_none() {
                m.remove(&ip);
            }
        }
    }

    pub fn is_locked(&self, ip: IpAddr) -> bool {
        let mut m = self.map.lock().unwrap();
        let Some(s) = m.get_mut(&ip) else { return false };
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
        let mut m = self.map.lock().unwrap();
        let s = m.entry(ip).or_default();
        while s.fails.front().is_some_and(|t| now.duration_since(*t) > lim.fail_window) {
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
