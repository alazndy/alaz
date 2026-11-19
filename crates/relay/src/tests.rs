use crate::{Limits, Relay, Transport};
use alaz_relay_proto::{Code, FromRelay, Refusal, RELAY_PROTOCOL, ToRelay, read_msg, write_msg};
use std::io::{self, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
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
    Limits { fail_delay: Duration::ZERO, max_fails_per_window: 10_000, ..Limits::default() }
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
    send(&mut c, ToRelay::Open { protocol: RELAY_PROTOCOL });
    let FromRelay::Opened { code } = recv(&mut c) else { panic!("not opened") };
    (c, code)
}

fn join_msg(code: Code) -> ToRelay {
    ToRelay::Join { protocol: RELAY_PROTOCOL, code }
}

/// Returns (joiner side, host tunnel side), both past `Paired`.
fn pair(a: SocketAddr, ctrl: &mut TcpStream, code: Code) -> (TcpStream, TcpStream) {
    let mut j = dial(a);
    send(&mut j, join_msg(code));
    let FromRelay::Incoming { id } = recv(ctrl) else { panic!("no incoming") };
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
    assert_eq!(join_reply(a, Code::generate()), FromRelay::Refused(Refusal::NoSuchCode));
}

#[test]
fn expired_and_unknown_codes_answer_alike() {
    let (r, a) = start(Limits { fail_delay: Duration::from_millis(100), max_fails_per_window: 10_000, ..Limits::default() });
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
    assert!(t_expired.abs_diff(t_unknown) < Duration::from_millis(80), "{t_expired:?} vs {t_unknown:?}");
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
    let FromRelay::Opened { code: new } = recv(&mut ctrl) else { panic!("no new code") };
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
    let (_r, a) = start(Limits { pair_timeout: Duration::from_millis(200), ..lenient() });
    let (mut ctrl, code) = open(a);
    let mut j = dial(a);
    send(&mut j, join_msg(code));
    assert!(matches!(recv(&mut ctrl), FromRelay::Incoming { .. }));
    assert_eq!(recv(&mut j), FromRelay::Refused(Refusal::Busy));
}

#[test]
fn per_ip_failures_lock_out_that_ip() {
    let (r, a) = start(Limits { max_fails_per_window: 3, fail_delay: Duration::ZERO, ..Limits::default() });
    let (_ctrl, code) = open(a);
    for _ in 0..3 {
        assert_eq!(join_reply(a, Code::generate()), FromRelay::Refused(Refusal::NoSuchCode));
    }
    assert_eq!(join_reply(a, Code::generate()), FromRelay::Refused(Refusal::NoSuchCode));
    assert_eq!(join_reply(a, code), FromRelay::Refused(Refusal::TooManyAttempts));
    assert_eq!(r.counters().lockouts, 1);
}

#[test]
fn per_ip_connection_cap() {
    let (_r, a) = start(Limits { max_conns_per_ip: 2, ..lenient() });
    let _c1 = dial(a);
    let _c2 = dial(a);
    let mut c3 = dial(a);
    assert_eq!(recv(&mut c3), FromRelay::Refused(Refusal::Busy));
}

#[test]
fn tunnel_throttles_but_does_not_cut_a_large_burst() {
    let (r, a) = start(Limits { tunnel_rate: 1 << 20, tunnel_burst: 16 << 20, ..lenient() });
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
    let (r, a) = start(Limits { abuse_cut: 100_000, tunnel_rate: 100 << 20, tunnel_burst: 100 << 20, ..lenient() });
    let (mut ctrl, code) = open(a);
    let (mut j, mut h) = pair(a, &mut ctrl, code);
    let block = vec![7u8; 65536];
    let _ = j.write_all(&block).and_then(|_| j.write_all(&block)).and_then(|_| j.write_all(&block));
    let mut sink = Vec::new();
    let _ = h.read_to_end(&mut sink);
    wait_for(|| r.counters().abuse_cuts == 1);
}

#[test]
fn bad_protocol_refused() {
    let (_r, a) = start(lenient());
    let mut c = dial(a);
    send(&mut c, ToRelay::Open { protocol: RELAY_PROTOCOL + 1 });
    assert_eq!(recv(&mut c), FromRelay::Refused(Refusal::BadProtocol));
    let mut j = dial(a);
    send(&mut j, ToRelay::Join { protocol: 0, code: Code::generate() });
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
        Err(e) => assert!(!matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut), "{e}"),
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
    write_msg(&mut look_alike, &ToRelay::Join { protocol: 99, code: Code::generate() }).unwrap();
    look_alike.extend_from_slice(&[0xFF; 16]);
    echo_once(&mut j, &mut h, &look_alike);
    echo_once(&mut h, &mut j, &look_alike);
}
