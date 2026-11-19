use crate::{Limits, Relay, Transport};
use alaz_relay_proto::{Code, FromRelay, RELAY_PROTOCOL, Refusal, ToRelay, read_msg, write_msg};
use std::io::{self, Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::{Duration, Instant};

struct Tcp(TcpStream);

impl Read for Tcp {
    fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
        self.0.read(b)
    }
}
impl Write for Tcp {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.0.write(b)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}
impl Transport for Tcp {
    fn peer_ip(&self) -> IpAddr {
        self.0.peer_addr().unwrap().ip()
    }
    fn shutdown(&self) {
        let _ = self.0.shutdown(std::net::Shutdown::Both);
    }
    fn set_read_timeout(&self, d: Option<Duration>) {
        let _ = self.0.set_read_timeout(d);
    }
    fn set_write_timeout(&self, d: Option<Duration>) {
        let _ = self.0.set_write_timeout(d);
    }
}

fn start(limits: Limits) -> (Arc<Relay>, SocketAddr) {
    let relay = Relay::new(limits);
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    let r = relay.clone();
    thread::spawn(move || {
        for s in l.incoming().flatten() {
            r.serve(Tcp(s));
        }
    });
    (relay, addr)
}

fn lenient() -> Limits {
    Limits {
        fail_delay: Duration::ZERO,
        max_fails_per_window: 10_000,
        ..Limits::default()
    }
}

fn dial(a: SocketAddr) -> TcpStream {
    let s = TcpStream::connect(a).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    s
}

fn send(s: &mut TcpStream, m: ToRelay) {
    write_msg(s, &m).unwrap();
}

fn recv(s: &mut TcpStream) -> FromRelay {
    read_msg(s).unwrap()
}

fn open(a: SocketAddr) -> (TcpStream, Code) {
    let mut c = dial(a);
    send(
        &mut c,
        ToRelay::Open {
            protocol: RELAY_PROTOCOL,
        },
    );
    let FromRelay::Opened { code } = recv(&mut c) else {
        panic!("not opened")
    };
    (c, code)
}

fn join_msg(code: Code) -> ToRelay {
    ToRelay::Join {
        protocol: RELAY_PROTOCOL,
        code,
    }
}

/// Returns (joiner side, host tunnel side), both past `Paired`.
fn pair(a: SocketAddr, ctrl: &mut TcpStream, code: Code) -> (TcpStream, TcpStream) {
    let mut j = dial(a);
    send(&mut j, join_msg(code));
    let FromRelay::Incoming { id } = recv(ctrl) else {
        panic!("no incoming")
    };
    let mut h = dial(a);
    send(&mut h, ToRelay::Accept { id });
    assert_eq!(recv(&mut h), FromRelay::Paired);
    assert_eq!(recv(&mut j), FromRelay::Paired);
    (j, h)
}

fn join_reply(a: SocketAddr, code: Code) -> FromRelay {
    let mut j = dial(a);
    send(&mut j, join_msg(code));
    recv(&mut j)
}

fn wait_for(mut f: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(10);
    while !f() {
        assert!(Instant::now() < end, "condition not reached in time");
        thread::sleep(Duration::from_millis(10));
    }
}

fn echo_once(from: &mut TcpStream, to: &mut TcpStream, bytes: &[u8]) {
    from.write_all(bytes).unwrap();
    let mut got = vec![0u8; bytes.len()];
    to.read_exact(&mut got).unwrap();
    assert_eq!(got, bytes);
}

#[test]
fn open_returns_valid_code() {
    let (_r, a) = start(lenient());
    let (_c, code) = open(a);
    assert_eq!(Code::parse(&code.to_string()), Some(code));
}

#[test]
fn join_with_code_pairs_and_splices_bytes_both_ways() {
    let (_r, a) = start(lenient());
    let (mut ctrl, code) = open(a);
    let (mut j, mut h) = pair(a, &mut ctrl, code);
    echo_once(&mut j, &mut h, b"hello host");
    echo_once(&mut h, &mut j, b"hello joiner");
}

#[test]
fn unknown_code_is_refused_no_such_code() {
    let (_r, a) = start(lenient());
    assert_eq!(
        join_reply(a, Code::generate()),
        FromRelay::Refused(Refusal::NoSuchCode)
    );
}

#[test]
fn expired_and_unknown_codes_answer_alike() {
    let (r, a) = start(Limits {
        fail_delay: Duration::from_millis(100),
        max_fails_per_window: 10_000,
        ..Limits::default()
    });
    let (ctrl, code) = open(a);
    drop(ctrl);
    wait_for(|| r.counters().rooms == 0);
    let t = Instant::now();
    let expired = join_reply(a, code);
    let t_expired = t.elapsed();
    let t = Instant::now();
    let unknown = join_reply(a, Code::generate());
    let t_unknown = t.elapsed();
    assert_eq!(expired, unknown);
    assert_eq!(expired, FromRelay::Refused(Refusal::NoSuchCode));
    assert!(t_expired >= Duration::from_millis(100) && t_unknown >= Duration::from_millis(100));
    assert!(
        t_expired.abs_diff(t_unknown) < Duration::from_millis(80),
        "{t_expired:?} vs {t_unknown:?}"
    );
}

#[test]
fn ninth_player_gets_room_full() {
    let (_r, a) = start(lenient());
    let (mut ctrl, code) = open(a);
    let mut held = Vec::new();
    for _ in 0..7 {
        held.push(pair(a, &mut ctrl, code));
    }
    assert_eq!(join_reply(a, code), FromRelay::Refused(Refusal::RoomFull));
}

#[test]
fn new_code_invalidates_old_code_and_keeps_tunnels() {
    let (_r, a) = start(lenient());
    let (mut ctrl, old) = open(a);
    let (mut j, mut h) = pair(a, &mut ctrl, old);
    send(&mut ctrl, ToRelay::NewCode);
    let FromRelay::Opened { code: new } = recv(&mut ctrl) else {
        panic!("no new code")
    };
    assert_ne!(new, old);
    assert_eq!(join_reply(a, old), FromRelay::Refused(Refusal::NoSuchCode));
    echo_once(&mut j, &mut h, b"still here");
    let _second = pair(a, &mut ctrl, new);
}

#[test]
fn host_control_drop_closes_room_but_not_open_tunnels() {
    let (r, a) = start(lenient());
    let (mut ctrl, code) = open(a);
    let (mut j, mut h) = pair(a, &mut ctrl, code);
    drop(ctrl);
    wait_for(|| r.counters().rooms == 0);
    assert_eq!(join_reply(a, code), FromRelay::Refused(Refusal::NoSuchCode));
    echo_once(&mut j, &mut h, b"tunnel lives");
    assert_eq!(r.counters().tunnels, 1);
}

#[test]
fn pairing_times_out_after_10s_when_host_never_accepts() {
    let (_r, a) = start(Limits {
        pair_timeout: Duration::from_millis(200),
        ..lenient()
    });
    let (mut ctrl, code) = open(a);
    let mut j = dial(a);
    send(&mut j, join_msg(code));
    assert!(matches!(recv(&mut ctrl), FromRelay::Incoming { .. }));
    assert_eq!(recv(&mut j), FromRelay::Refused(Refusal::Busy));
}

#[test]
fn per_ip_failures_lock_out_that_ip() {
    let (r, a) = start(Limits {
        max_fails_per_window: 3,
        fail_delay: Duration::ZERO,
        ..Limits::default()
    });
    let (_ctrl, code) = open(a);
    for _ in 0..3 {
        assert_eq!(
            join_reply(a, Code::generate()),
            FromRelay::Refused(Refusal::NoSuchCode)
        );
    }
    assert_eq!(
        join_reply(a, Code::generate()),
        FromRelay::Refused(Refusal::NoSuchCode)
    );
    // a locked IP is closed at accept, with no reply and no thread
    let spawned = r.spawned.load(Ordering::Relaxed);
    assert!(closed_silently(a));
    assert_eq!(r.spawned.load(Ordering::Relaxed), spawned);
    let _ = code;
    assert_eq!(r.counters().lockouts, 1);
}

#[test]
fn per_ip_connection_cap() {
    let (_r, a) = start(Limits {
        max_conns_per_ip: 2,
        ..lenient()
    });
    let _c1 = dial(a);
    let _c2 = dial(a);
    wait_for(|| _r.spawned.load(Ordering::Relaxed) == 2);
    // over the cap: closed with no reply and no thread
    assert!(closed_silently(a));
    assert_eq!(_r.spawned.load(Ordering::Relaxed), 2);
}

#[test]
fn tunnel_throttles_but_does_not_cut_a_large_burst() {
    let (r, a) = start(Limits {
        tunnel_rate: 1 << 20,
        tunnel_burst: 16 << 20,
        ..lenient()
    });
    let (mut ctrl, code) = open(a);
    let (j, mut h) = pair(a, &mut ctrl, code);
    const N: usize = 8 << 20;
    let mut w = j.try_clone().unwrap();
    let writer = thread::spawn(move || {
        let block: Vec<u8> = (0..65536u32).map(|i| (i % 251) as u8).collect();
        for _ in 0..N / block.len() {
            w.write_all(&block).unwrap();
        }
    });
    let mut got = 0usize;
    let mut buf = vec![0u8; 65536];
    while got < N {
        let n = h.read(&mut buf).unwrap();
        assert!(n > 0, "tunnel cut after {got} bytes");
        for (i, b) in buf[..n].iter().enumerate() {
            assert_eq!(*b, ((got + i) % 65536 % 251) as u8);
        }
        got += n;
    }
    writer.join().unwrap();
    assert_eq!(r.counters().abuse_cuts, 0);
    drop(j);
}

#[test]
fn abuse_beyond_cut_is_closed_and_counted() {
    let (r, a) = start(Limits {
        abuse_cut: 100_000,
        tunnel_rate: 100 << 20,
        tunnel_burst: 100 << 20,
        ..lenient()
    });
    let (mut ctrl, code) = open(a);
    let (mut j, mut h) = pair(a, &mut ctrl, code);
    let block = vec![7u8; 65536];
    let _ = j
        .write_all(&block)
        .and_then(|_| j.write_all(&block))
        .and_then(|_| j.write_all(&block));
    let mut sink = Vec::new();
    let _ = h.read_to_end(&mut sink);
    wait_for(|| r.counters().abuse_cuts == 1);
}

#[test]
fn bad_protocol_refused() {
    let (_r, a) = start(lenient());
    let mut c = dial(a);
    send(
        &mut c,
        ToRelay::Open {
            protocol: RELAY_PROTOCOL + 1,
        },
    );
    assert_eq!(recv(&mut c), FromRelay::Refused(Refusal::BadProtocol));
    let mut j = dial(a);
    send(
        &mut j,
        ToRelay::Join {
            protocol: 0,
            code: Code::generate(),
        },
    );
    assert_eq!(recv(&mut j), FromRelay::Refused(Refusal::BadProtocol));
}

#[test]
fn oversize_first_frame_closes_connection() {
    let (_r, a) = start(lenient());
    let mut c = dial(a);
    c.write_all(&5000u32.to_le_bytes()).unwrap();
    let mut rest = Vec::new();
    match c.read_to_end(&mut rest) {
        Ok(_) => {}
        Err(e) => assert!(
            !matches!(
                e.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
            ),
            "{e}"
        ),
    }
    assert!(rest.is_empty());
}

#[test]
fn splice_never_parses_bytes() {
    let (_r, a) = start(lenient());
    let (mut ctrl, code) = open(a);
    let (mut j, mut h) = pair(a, &mut ctrl, code);
    let mut look_alike = Vec::new();
    write_msg(&mut look_alike, &ToRelay::NewCode).unwrap();
    write_msg(
        &mut look_alike,
        &ToRelay::Join {
            protocol: 99,
            code: Code::generate(),
        },
    )
    .unwrap();
    look_alike.extend_from_slice(&[0xFF; 16]);
    echo_once(&mut j, &mut h, &look_alike);
    echo_once(&mut h, &mut j, &look_alike);
}

#[test]
fn stuck_receiver_tunnel_is_reaped_and_slots_released() {
    let (r, a) = start(Limits {
        write_stall: Duration::from_millis(300),
        tunnel_rate: 1 << 30,
        tunnel_burst: 1 << 30,
        ..lenient()
    });
    let (mut ctrl, code) = open(a);
    let (j, mut h) = pair(a, &mut ctrl, code);
    drop(ctrl);
    // the joiner never reads; the host floods until the relay gives up on the tunnel
    let flood = thread::spawn(move || {
        let block = vec![1u8; 65536];
        while h.write_all(&block).is_ok() {}
    });
    wait_for(|| {
        r.counters().tunnels == 0
            && r.counters().rooms == 0
            && r.ips.conns(Ipv4Addr::LOCALHOST.into()) == 0
    });
    flood.join().unwrap();
    assert_eq!(r.active_joins.load(std::sync::atomic::Ordering::Relaxed), 0);
    drop(j);
}

#[test]
fn silent_control_connection_is_dropped_and_pinging_one_is_kept() {
    let (r, a) = start(Limits {
        control_idle: Duration::from_millis(300),
        ..lenient()
    });
    let (_silent, silent_code) = open(a);
    let (mut pinger, ping_code) = open(a);
    let end = Instant::now() + Duration::from_millis(900);
    while Instant::now() < end {
        send(&mut pinger, ToRelay::Ping);
        thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(r.counters().rooms, 1);
    assert_eq!(
        join_reply(a, silent_code),
        FromRelay::Refused(Refusal::NoSuchCode)
    );
    let mut j = dial(a);
    send(&mut j, join_msg(ping_code));
    assert!(matches!(recv(&mut pinger), FromRelay::Incoming { .. }));
}

#[test]
fn first_message_deadline_covers_the_whole_frame() {
    let (_r, a) = start(Limits {
        first_frame_timeout: Duration::from_millis(300),
        ..lenient()
    });
    let mut c = dial(a);
    let t = Instant::now();
    // one byte at a time: each read alone is well inside the limit
    for b in 8u32.to_le_bytes() {
        if c.write_all(&[b]).is_err() {
            break;
        }
        thread::sleep(Duration::from_millis(150));
    }
    let mut rest = Vec::new();
    let _ = c.read_to_end(&mut rest);
    assert!(t.elapsed() < Duration::from_secs(3));
    assert!(rest.is_empty());
}

#[test]
fn accept_is_exempt_from_lockout() {
    let (_r, a) = start(Limits {
        max_fails_per_window: 1,
        fail_delay: Duration::ZERO,
        ..Limits::default()
    });
    let (mut ctrl, code) = open(a);
    for _ in 0..3 {
        miss(a);
    }
    // new sockets from the locked IP are closed at accept; connections already open keep working
    assert!(closed_silently(a));
    let _ = code;
    // the host's own control connection still works while the IP is locked
    send(&mut ctrl, ToRelay::NewCode);
    assert!(matches!(recv(&mut ctrl), FromRelay::Opened { .. }));
}

#[test]
fn small_burst_and_low_rate_actually_delay() {
    let (_r, a) = start(Limits {
        tunnel_rate: 1 << 20,
        tunnel_burst: 64 << 10,
        ..lenient()
    });
    let (mut ctrl, code) = open(a);
    let (mut j, mut h) = pair(a, &mut ctrl, code);
    const N: usize = 512 << 10;
    let t = Instant::now();
    let w = thread::spawn(move || j.write_all(&vec![9u8; N]).map(|_| j));
    let mut got = vec![0u8; N];
    h.read_exact(&mut got).unwrap();
    let _j = w.join().unwrap().unwrap();
    // (512 - 64) KiB at 1 MiB/s is about 440 ms; accept anything above half of that
    assert!(
        t.elapsed() >= Duration::from_millis(200),
        "{:?}",
        t.elapsed()
    );
}

#[test]
fn ipv6_addresses_in_one_slash_64_share_a_lockout_and_stale_entries_vanish() {
    let lim = Limits {
        max_fails_per_window: 1,
        fail_window: Duration::from_millis(100),
        ..Limits::default()
    };
    let t = crate::room::IpTable::new(lim.fail_window);
    let a: IpAddr = "2001:db8:1:2::1".parse().unwrap();
    let b: IpAddr = "2001:db8:1:2:ffff::9".parse().unwrap();
    let other: IpAddr = "2001:db8:1:3::1".parse().unwrap();
    // 2 fails in the /64 lock it; the /48 holds 2 of its 4 allowed, so a sibling /64 is still free
    assert!(!t.record_fail(a, &lim));
    assert!(t.record_fail(b, &lim));
    assert!(t.is_locked(a) && t.is_locked(b) && !t.is_locked(other));
    let c: IpAddr = "192.0.2.1".parse().unwrap();
    for _ in 0..3 {
        t.record_fail(c, &lim);
        t.record_fail(other, &lim);
    }
    assert!(t.len() >= 2);
    // lockout lasts 300 s, so use a fresh table for the staleness check
    let t = crate::room::IpTable::new(lim.fail_window);
    let big = Limits {
        max_fails_per_window: 1000,
        ..lim.clone()
    };
    t.record_fail(c, &big);
    t.record_fail(other, &big);
    // c: one IPv4 entry; other: its /64 and its /48
    assert_eq!(t.len(), 3);
    thread::sleep(Duration::from_millis(250));
    assert!(t.try_add_conn(a, 5));
    t.drop_conn(a);
    assert_eq!(t.len(), 0);
}

/// True when a fresh connection is closed by the relay without a single byte coming back.
fn closed_silently(a: SocketAddr) -> bool {
    let mut s = TcpStream::connect(a).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let mut buf = Vec::new();
    match s.read_to_end(&mut buf) {
        Ok(_) => buf.is_empty(),
        Err(e) => {
            !matches!(
                e.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
            ) && buf.is_empty()
        }
    }
}

#[test]
fn over_cap_connections_are_closed_without_a_thread() {
    let (r, a) = start(Limits {
        max_total_conns: 3,
        ..lenient()
    });
    let held: Vec<TcpStream> = (0..3).map(|_| dial(a)).collect();
    wait_for(|| r.spawned.load(Ordering::Relaxed) == 3);
    for _ in 0..10 {
        assert!(closed_silently(a));
    }
    assert_eq!(
        r.spawned.load(Ordering::Relaxed),
        3,
        "over-cap sockets got a thread"
    );
    assert_eq!(r.total_conns.load(Ordering::Relaxed), 3);
    drop(held);
    wait_for(|| r.total_conns.load(Ordering::Relaxed) == 0);
    let (_c, _code) = open(a);
}

#[test]
fn refused_thread_drops_the_connection_and_frees_every_slot() {
    // a stack no OS can give: spawn returns Err, which must not panic
    let r = Relay::new(Limits {
        stack_size: 1 << 50,
        ..lenient()
    });
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    let mut peer = TcpStream::connect(addr).unwrap();
    let (s, _) = l.accept().unwrap();
    r.serve(Tcp(s));
    assert_eq!(r.total_conns.load(Ordering::Relaxed), 0);
    assert_eq!(r.ips.conns(Ipv4Addr::LOCALHOST.into()), 0);
    peer.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let mut buf = Vec::new();
    assert!(peer.read_to_end(&mut buf).is_ok() && buf.is_empty());
}

#[test]
fn locked_ip_is_closed_at_accept_not_after_the_first_frame() {
    let (r, a) = start(Limits {
        max_fails_per_window: 1,
        ..lenient()
    });
    for _ in 0..3 {
        miss(a);
    }
    let n = r.spawned.load(Ordering::Relaxed);
    for _ in 0..5 {
        assert!(closed_silently(a));
    }
    assert_eq!(r.spawned.load(Ordering::Relaxed), n);
}

#[test]
fn idle_tunnel_is_closed_and_slots_released() {
    let (r, a) = start(Limits {
        tunnel_idle: Duration::from_millis(400),
        ..lenient()
    });
    let (mut ctrl, code) = open(a);
    let (mut j, mut h) = pair(a, &mut ctrl, code);
    // traffic keeps it alive well past the idle limit
    for _ in 0..8 {
        echo_once(&mut j, &mut h, b"x");
        thread::sleep(Duration::from_millis(100));
    }
    assert_eq!(r.counters().tunnels, 1);
    // then silence: the relay closes both ends and frees the places
    wait_for(|| r.counters().tunnels == 0);
    let mut buf = Vec::new();
    assert!(j.read_to_end(&mut buf).is_ok());
    assert_eq!(r.active_joins.load(Ordering::Relaxed), 0);
    drop(h);
}

#[test]
fn rooms_per_ip_are_capped() {
    let (r, a) = start(Limits {
        max_rooms_per_ip: 2,
        ..lenient()
    });
    let _a = open(a);
    let _b = open(a);
    let mut c = dial(a);
    send(
        &mut c,
        ToRelay::Open {
            protocol: RELAY_PROTOCOL,
        },
    );
    assert_eq!(recv(&mut c), FromRelay::Refused(Refusal::Busy));
    assert_eq!(r.counters().rooms, 2);
}

#[test]
fn tunnels_per_ip_are_capped_and_released() {
    let (r, a) = start(Limits {
        max_tunnels_per_ip: 2,
        ..lenient()
    });
    let (mut ctrl, code) = open(a);
    let p1 = pair(a, &mut ctrl, code);
    let _p2 = pair(a, &mut ctrl, code);
    assert_eq!(join_reply(a, code), FromRelay::Refused(Refusal::Busy));
    drop(p1);
    wait_for(|| r.counters().tunnels == 1);
    let _p3 = pair(a, &mut ctrl, code);
}

#[test]
fn global_join_budget_keeps_a_reserve_for_clean_ips() {
    let r = Relay::new(Limits {
        global_attempts_per_sec: 4,
        reserved_attempts_per_sec: 2,
        ..Limits::default()
    });
    assert!(r.attempt_allowed(true));
    assert!(r.attempt_allowed(true));
    assert!(
        !r.attempt_allowed(true),
        "failing IPs stop at budget minus reserve"
    );
    assert!(r.attempt_allowed(false));
    assert!(r.attempt_allowed(false));
    assert!(!r.attempt_allowed(false), "the whole budget is still 4");
}

#[test]
fn default_limits_match_the_documented_numbers() {
    let l = Limits::default();
    assert_eq!(l.global_attempts_per_sec, 20);
    assert_eq!(l.max_rooms_per_ip, 4);
    assert_eq!(l.tunnel_idle, Duration::from_secs(120));
    // two sockets per tunnel plus one per room must fit under the socket cap
    assert!(l.max_tunnels * 2 + l.max_rooms < l.max_total_conns);
}

#[test]
fn ipv6_slash_48_aggregates_connections_and_failures() {
    use crate::room::{IpTable, Kind};
    let t = IpTable::new(Duration::from_secs(60));
    // eight different /64 networks of one /48, cap 2 per /64 and 8 per /48
    let nets: Vec<IpAddr> = (0..16u16)
        .map(|i| format!("2001:db8:7:{i}::1").parse().unwrap())
        .collect();
    let mut taken = 0;
    for ip in &nets {
        if t.acquire(*ip, Kind::Conn, 2) {
            taken += 1;
        }
    }
    assert_eq!(taken, 8, "the /48 bucket caps rotation across /64s");
    // the narrower bucket still wins inside one /64
    let t = IpTable::new(Duration::from_secs(60));
    let same = nets[0];
    assert!(t.acquire(same, Kind::Conn, 2) && t.acquire(same, Kind::Conn, 2));
    assert!(!t.acquire(same, Kind::Conn, 2));
    // failures rotate across /64s but still lock the /48: limit 3 per /64, 12 per /48
    let lim = Limits {
        max_fails_per_window: 3,
        ..Limits::default()
    };
    let t = IpTable::new(lim.fail_window);
    let mut locked = false;
    for ip in nets.iter().cycle().take(13) {
        locked |= t.record_fail(*ip, &lim);
    }
    assert!(locked);
    assert!(t.is_locked("2001:db8:7:ffff::1".parse().unwrap()));
    assert!(!t.is_locked("2001:db8:8::1".parse().unwrap()));
}

#[test]
fn ip_table_sweep_is_amortized() {
    let t = crate::room::IpTable::new(Duration::from_secs(60));
    for i in 0..5000u32 {
        let ip = IpAddr::V4(Ipv4Addr::from(0x0a00_0000 + i));
        assert!(t.try_add_conn(ip, 4));
    }
    assert!(t.sweeps() <= 1, "swept {} times for 5000 calls", t.sweeps());
    assert_eq!(t.len(), 5000);
}

/// A wrong-code attempt; the relay may already have closed the socket once the IP is locked.
fn miss(a: SocketAddr) {
    let mut j = dial(a);
    let _ = write_msg(&mut j, &join_msg(Code::generate()));
    let _ = read_msg::<FromRelay, _>(&mut j);
}
