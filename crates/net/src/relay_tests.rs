//! Host and join through an in-process relay on loopback, with a throw-away test certificate.
use super::tests::{host_world, run, settle, setup, verify_world_replacement};
use super::*;
use crate::relay_client::TEST_ROOTS;
use alaz_relay_proto::Code;
use alaz_relay::tls::{Reloadable, accept_loop};
use alaz_relay::{Limits, Relay};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;

struct Env {
    addr: String,
    dir: PathBuf,
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn env(limits: Limits) -> Env {
    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("alaz-net-relay-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let ck = rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
    let (cert, key) = (dir.join("cert.pem"), dir.join("key.pem"));
    std::fs::write(&cert, ck.cert.pem()).unwrap();
    std::fs::write(&key, ck.key_pair.serialize_pem()).unwrap();
    TEST_ROOTS.lock().unwrap().push(ck.cert.der().clone());
    let certs = Reloadable::new(&cert, &key).unwrap();
    let relay = Relay::new(Limits { fail_delay: Duration::ZERO, ..limits });
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = format!("localhost:{}", l.local_addr().unwrap().port());
    thread::spawn(move || accept_loop(l, relay, Arc::clone(&certs)));
    Env { addr, dir }
}

fn open(host: &mut Host, env: &Env) -> Code {
    host.open_relay(&env.addr).unwrap();
    wait_code(host)
}

fn wait_code(host: &Host) -> Code {
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        match host.relay_status() {
            RelayStatus::Open(c) => return c,
            RelayStatus::Failed(e) => panic!("relay failed: {e}"),
            _ => assert!(Instant::now() < end, "relay never opened"),
        }
        thread::sleep(Duration::from_millis(5));
    }
}

/// Join through the relay while the host keeps ticking, like the game loop does.
fn join(host: &mut Host, hw: &mut World, env: &Env, code: &str) -> Result<(Client, World), NetError> {
    let (code, addr) = (code.to_string(), env.addr.clone());
    let t = thread::spawn(move || Client::connect_code_via(&addr, &code, Duration::from_secs(10)));
    while !t.is_finished() {
        host.poll(hw);
        let c = host.turn(hw, vec![]);
        hw.step(&c);
        thread::sleep(Duration::from_millis(1));
    }
    t.join().unwrap()
}

#[test]
fn connect_code_through_relay_stays_in_sync_across_world_replacement() {
    let env = env(Limits::default());
    let mut host = Host::bind("127.0.0.1:0").unwrap();
    let mut hw = host_world();
    let code = open(&mut host, &env);
    let (mut client, mut cw) = join(&mut host, &mut hw, &env, &code.to_string()).unwrap();
    assert_eq!(host.players(), 2);
    let ka = socket2::SockRef::from(client.conn.stream.tcp());
    assert!(ka.keepalive().unwrap(), "keepalive not set on the relay client socket");
    run(&mut host, &mut hw, &mut client, &mut cw, 300, |_, _, _, _, _| {});
    settle(&mut host, &mut hw, &mut client, &mut cw);
    assert_eq!(cw.tick, hw.tick);
    assert_eq!(cw.hash(), hw.hash());
    verify_world_replacement(host, client, cw);
}

#[test]
fn two_clients_join_one_host() {
    let env = env(Limits::default());
    let mut host = Host::bind("127.0.0.1:0").unwrap();
    let mut hw = host_world();
    let code = open(&mut host, &env).to_string();
    let (mut c1, mut w1) = join(&mut host, &mut hw, &env, &code).unwrap();
    let (mut c2, mut w2) = join(&mut host, &mut hw, &env, &code).unwrap();
    assert_eq!(host.players(), 3);
    assert_ne!(c1.agent, c2.agent);
    run(&mut host, &mut hw, &mut c2, &mut w2, 200, |_, _, _, _, _| {});
    settle(&mut host, &mut hw, &mut c2, &mut w2);
    settle(&mut host, &mut hw, &mut c1, &mut w1);
    assert_eq!(w1.hash(), hw.hash());
    assert_eq!(w2.hash(), hw.hash());
}

#[test]
fn wrong_code_gives_turkish_error() {
    let env = env(Limits::default());
    let missing = Client::connect_code_via(&env.addr, "AAAA-AAAA", Duration::from_secs(5)).err().unwrap();
    assert_eq!(missing.0, "kod geçersiz");
    let garbage = Client::connect_code_via(&env.addr, "xx", Duration::from_secs(5)).err().unwrap();
    assert_eq!(garbage.0, "kod geçersiz");
}

#[test]
fn room_full_error_for_ninth() {
    let env = env(Limits::default());
    let mut host = Host::bind("127.0.0.1:0").unwrap();
    let mut hw = host_world();
    let code = open(&mut host, &env).to_string();
    let mut clients = Vec::new();
    for _ in 0..7 {
        clients.push(join(&mut host, &mut hw, &env, &code).unwrap());
    }
    assert_eq!(host.players(), 8);
    let err = join(&mut host, &mut hw, &env, &code).err().unwrap();
    assert_eq!(err.0, "oda dolu");
    assert_eq!(host.players(), 8, "host unaffected");
}

#[test]
fn client_sees_disconnect_when_host_drops() {
    let env = env(Limits::default());
    let mut host = Host::bind("127.0.0.1:0").unwrap();
    let mut hw = host_world();
    let code = open(&mut host, &env).to_string();
    let (mut client, _cw) = join(&mut host, &mut hw, &env, &code).unwrap();
    assert!(client.is_connected());
    drop(host);
    let end = Instant::now() + Duration::from_secs(5);
    while client.is_connected() {
        client.poll();
        assert!(Instant::now() < end, "client never noticed");
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn host_status_reports_failed_when_relay_unreachable() {
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let mut host = Host::bind("127.0.0.1:0").unwrap();
    let start = Instant::now();
    host.open_relay(&format!("localhost:{port}")).unwrap();
    loop {
        match host.relay_status() {
            RelayStatus::Failed(e) => {
                assert!(!e.is_empty());
                break;
            }
            RelayStatus::Open(_) => panic!("cannot be open"),
            _ => assert!(start.elapsed() < Duration::from_secs(3), "no failure within 3 s"),
        }
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn relay_new_code_rejects_old_code() {
    let env = env(Limits::default());
    let mut host = Host::bind("127.0.0.1:0").unwrap();
    let mut hw = host_world();
    let old = open(&mut host, &env);
    host.relay_new_code();
    let new = wait_code(&host);
    assert_ne!(old, new);
    let err = join(&mut host, &mut hw, &env, &old.to_string()).err().unwrap();
    assert_eq!(err.0, "kod geçersiz");
    assert!(join(&mut host, &mut hw, &env, &new.to_string()).is_ok());
}

#[test]
fn relay_close_keeps_existing_players() {
    let env = env(Limits::default());
    let mut host = Host::bind("127.0.0.1:0").unwrap();
    let mut hw = host_world();
    let code = open(&mut host, &env).to_string();
    let (mut client, mut cw) = join(&mut host, &mut hw, &env, &code).unwrap();
    host.relay_close();
    assert_eq!(host.relay_status(), RelayStatus::Closed);
    run(&mut host, &mut hw, &mut client, &mut cw, 200, |_, _, _, _, _| {});
    settle(&mut host, &mut hw, &mut client, &mut cw);
    assert_eq!(host.players(), 2);
    assert!(client.is_connected());
    assert_eq!(cw.hash(), hw.hash());
    let err = join(&mut host, &mut hw, &env, &code).err().expect("joined after close");
    assert_eq!(err.0, "kod geçersiz");
}

#[test]
fn control_pings_keep_the_room_alive() {
    let env = env(Limits { control_idle: Duration::from_millis(1200), ..Limits::default() });
    let mut host = Host::bind("127.0.0.1:0").unwrap();
    let mut hw = host_world();
    host.open_relay_with(&env.addr, Duration::from_millis(200)).unwrap();
    let code = wait_code(&host).to_string();
    thread::sleep(Duration::from_millis(2600));
    assert_eq!(host.relay_status(), RelayStatus::Open(code.parse_code()));
    assert!(join(&mut host, &mut hw, &env, &code).is_ok());
}

#[test]
fn connect_code_obeys_timeout_when_relay_is_silent() {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = format!("localhost:{}", l.local_addr().unwrap().port());
    thread::spawn(move || {
        let mut held = Vec::new();
        for s in l.incoming().flatten() {
            held.push(s);
        }
    });
    let start = Instant::now();
    let err = Client::connect_code_via(&addr, "AAAA-AAAA", Duration::from_millis(800)).err().unwrap();
    assert!(start.elapsed() < Duration::from_millis(2000), "took {:?}", start.elapsed());
    assert!(!err.0.is_empty());
}

#[test]
fn lan_host_and_client_still_work_unchanged() {
    let (mut host, mut hw, mut client, mut cw) = setup();
    run(&mut host, &mut hw, &mut client, &mut cw, 100, |_, _, _, _, _| {});
    settle(&mut host, &mut hw, &mut client, &mut cw);
    assert_eq!(cw.hash(), hw.hash());
    assert_eq!(host.relay_status(), RelayStatus::Closed);
}

trait ParseCode {
    fn parse_code(&self) -> Code;
}
impl ParseCode for String {
    fn parse_code(&self) -> Code {
        Code::parse(self).unwrap()
    }
}

#[test]
fn dial_threads_are_capped_and_released() {
    use crate::relay_client::spawn_capped;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;
    let live = Arc::new(AtomicUsize::new(0));
    let (release, gate) = mpsc::channel::<()>();
    let gate = Arc::new(Mutex::new(gate));
    for i in 0..3 {
        let gate = gate.clone();
        assert!(spawn_capped(&live, 3, move || {
            let _ = gate.lock().unwrap().recv();
            let _ = i;
        }));
    }
    assert!(!spawn_capped(&live, 3, || {}), "a fourth dial over the cap must be dropped");
    assert_eq!(live.load(Ordering::SeqCst), 3);
    drop(release);
    let end = Instant::now() + Duration::from_secs(5);
    while live.load(Ordering::SeqCst) != 0 {
        assert!(Instant::now() < end, "slots not released");
        thread::sleep(Duration::from_millis(5));
    }
    assert!(spawn_capped(&live, 3, || {}));
}
