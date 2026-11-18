//! LAN discovery over UDP. A joiner broadcasts a small query; every host answers with a unicast reply.
//! Only the host binds a fixed port, so several finders can run on one machine.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

pub const DISCOVERY_PORT: u16 = 7778;
const MAGIC: [u8; 4] = *b"ALAZ";
const STALE: Duration = Duration::from_secs(3);
const QUERY_EVERY: Duration = Duration::from_secs(1);
const MAX_LABEL: usize = 64;

#[derive(Serialize, Deserialize)]
struct Query {
    magic: [u8; 4],
    protocol: u32,
}

#[derive(Serialize, Deserialize)]
struct Reply {
    magic: [u8; 4],
    protocol: u32,
    tcp_port: u16,
    players: u32,
    label: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Found {
    /// Host IP (taken from the reply's source) plus its TCP port.
    pub addr: SocketAddr,
    pub players: u32,
    pub label: String,
}

/// Host side: answers queries.
pub struct Beacon {
    sock: UdpSocket,
}

impl Beacon {
    pub fn bind(port: u16) -> io::Result<Beacon> {
        let sock = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, port))?;
        sock.set_nonblocking(true)?;
        Ok(Beacon { sock })
    }

    pub fn local_port(&self) -> u16 {
        self.sock.local_addr().map(|a| a.port()).unwrap_or(0)
    }

    /// Reply to every pending query. Never blocks. Send errors are dropped: the next query retries.
    pub fn answer(&self, tcp_port: u16, players: u32, label: &str) {
        let mut buf = [0u8; 512];
        while let Ok((n, from)) = self.sock.recv_from(&mut buf) {
            let Ok(q) = postcard::from_bytes::<Query>(&buf[..n]) else { continue };
            if q.magic != MAGIC || q.protocol != crate::PROTOCOL {
                continue;
            }
            let reply = Reply {
                magic: MAGIC,
                protocol: crate::PROTOCOL,
                tcp_port,
                players,
                label: label.chars().take(MAX_LABEL).collect(),
            };
            if let Ok(bytes) = postcard::to_allocvec(&reply) {
                let _ = self.sock.send_to(&bytes, from);
            }
        }
    }
}

/// Joiner side: queries and collects replies.
pub struct Finder {
    sock: UdpSocket,
    target: SocketAddr,
    last_query: Option<Instant>,
    seen: HashMap<SocketAddr, (Found, Instant)>,
}

impl Finder {
    pub fn new() -> io::Result<Finder> {
        Finder::with_target(SocketAddr::from((Ipv4Addr::BROADCAST, DISCOVERY_PORT)))
    }

    pub fn with_target(target: SocketAddr) -> io::Result<Finder> {
        let sock = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
        sock.set_nonblocking(true)?;
        sock.set_broadcast(true)?;
        Ok(Finder { sock, target, last_query: None, seen: HashMap::new() })
    }

    pub fn poll(&mut self) {
        let now = Instant::now();
        if self.last_query.is_none_or(|t| now.duration_since(t) >= QUERY_EVERY) {
            self.last_query = Some(now);
            if let Ok(bytes) = postcard::to_allocvec(&Query { magic: MAGIC, protocol: crate::PROTOCOL }) {
                let _ = self.sock.send_to(&bytes, self.target);
            }
        }
        let mut buf = [0u8; 512];
        while let Ok((n, from)) = self.sock.recv_from(&mut buf) {
            let Ok(r) = postcard::from_bytes::<Reply>(&buf[..n]) else { continue };
            if r.magic != MAGIC || r.protocol != crate::PROTOCOL {
                continue;
            }
            let addr = SocketAddr::new(from.ip(), r.tcp_port);
            let label: String = r.label.chars().take(MAX_LABEL).collect();
            self.seen.insert(addr, (Found { addr, players: r.players, label }, now));
        }
        // forged replies with many source ports would otherwise grow this map for as long as the screen is open
        self.seen.retain(|_, (_, t)| now.saturating_duration_since(*t) < STALE);
    }

    /// Hosts that answered within the last 3 s, sorted by address.
    pub fn found(&self) -> Vec<Found> {
        self.found_at(Instant::now())
    }

    fn found_at(&self, now: Instant) -> Vec<Found> {
        let mut out: Vec<Found> =
            self.seen.values().filter(|(_, t)| now.saturating_duration_since(*t) < STALE).map(|(f, _)| f.clone()).collect();
        out.sort_by_key(|f| f.addr);
        out
    }
}

/// This machine's LAN address. Connecting a UDP socket sends nothing; it only makes the OS pick a route.
pub fn lan_ip() -> Option<IpAddr> {
    let sock = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    sock.connect((Ipv4Addr::new(10, 255, 255, 255), 9)).ok()?;
    let ip = sock.local_addr().ok()?.ip();
    (!ip.is_loopback() && !ip.is_unspecified()).then_some(ip)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    fn local(port: u16) -> SocketAddr {
        SocketAddr::from((Ipv4Addr::LOCALHOST, port))
    }

    fn find_one(finder: &mut Finder, beacon: &Beacon) {
        let start = Instant::now();
        while finder.found().is_empty() && start.elapsed() < Duration::from_secs(2) {
            finder.poll();
            beacon.answer(7777, 2, "Yuva 1 · Gün 3");
            finder.poll();
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn finder_sees_a_host_on_localhost() {
        let beacon = Beacon::bind(0).unwrap();
        let mut finder = Finder::with_target(local(beacon.local_port())).unwrap();
        find_one(&mut finder, &beacon);
        assert_eq!(finder.found(), vec![Found { addr: local(7777), players: 2, label: "Yuva 1 · Gün 3".into() }]);
    }

    #[test]
    fn discovery_ignores_garbage() {
        let beacon = Beacon::bind(0).unwrap();
        let mut finder = Finder::with_target(local(beacon.local_port())).unwrap();
        let finder_addr = local(finder.sock.local_addr().unwrap().port());
        let other = UdpSocket::bind("127.0.0.1:0").unwrap();
        let foreign = postcard::to_allocvec(&Reply { magic: *b"XXXX", protocol: crate::PROTOCOL, tcp_port: 1, players: 1, label: "x".into() }).unwrap();
        let wrong_proto = postcard::to_allocvec(&Reply { magic: MAGIC, protocol: crate::PROTOCOL + 1, tcp_port: 1, players: 1, label: "x".into() }).unwrap();
        for pkt in [&b"hello"[..], &foreign, &wrong_proto] {
            other.send_to(pkt, finder_addr).unwrap();
            other.send_to(pkt, local(beacon.local_port())).unwrap();
        }
        thread::sleep(Duration::from_millis(50));
        beacon.answer(7777, 1, "x");
        thread::sleep(Duration::from_millis(50));
        finder.poll();
        assert!(finder.found().is_empty());
        other.set_nonblocking(true).unwrap();
        let mut buf = [0u8; 512];
        assert!(other.recv_from(&mut buf).is_err(), "the beacon did not answer garbage");
    }

    #[test]
    fn finder_forgets_stale_hosts() {
        let beacon = Beacon::bind(0).unwrap();
        let mut finder = Finder::with_target(local(beacon.local_port())).unwrap();
        find_one(&mut finder, &beacon);
        assert_eq!(finder.found().len(), 1);
        assert!(finder.found_at(Instant::now() + Duration::from_secs(4)).is_empty());
    }
}
