//! The relay side of `alaz-net`: dialing the relay over TLS and the host's background control thread.
//! Everything is std threads and channels; the game loop only ever does non-blocking `try_recv` and mutex reads.
use crate::NetError;
use crate::stream::Stream;
use alaz_relay_proto::{Code, FromRelay, MAX_RELAY_FRAME, RELAY_PROTOCOL, Refusal, ToRelay, read_msg, write_msg};
use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// The relay drops a room after 60 s without a frame; four pings fit in that window.
pub(crate) const PING_EVERY: Duration = Duration::from_secs(15);
const STEP_WAIT: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(100);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RelayStatus {
    Connecting,
    Open(Code),
    Failed(String),
    Closed,
}

/// Certificates a test relay presents. Tests only: production trusts `webpki-roots` and nothing else.
#[cfg(test)]
pub(crate) static TEST_ROOTS: Mutex<Vec<rustls::pki_types::CertificateDer<'static>>> = Mutex::new(Vec::new());

fn build_config() -> Arc<ClientConfig> {
    #[allow(unused_mut)] // only tests add roots
    let mut roots = RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() };
    #[cfg(test)]
    for c in TEST_ROOTS.lock().unwrap().iter() {
        roots.add(c.clone()).expect("test certificate");
    }
    let cfg = ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .expect("tls versions")
        .with_root_certificates(roots)
        .with_no_client_auth();
    Arc::new(cfg)
}

fn tls_config() -> Arc<ClientConfig> {
    #[cfg(not(test))]
    {
        static CONFIG: std::sync::OnceLock<Arc<ClientConfig>> = std::sync::OnceLock::new();
        CONFIG.get_or_init(build_config).clone()
    }
    // tests register their certificate after earlier tests already ran
    #[cfg(test)]
    build_config()
}

fn err(msg: &str) -> NetError {
    NetError(msg.into())
}

fn io_err(e: io::Error) -> NetError {
    match e.kind() {
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock => err("röle yanıt vermedi"),
        _ => err("bağlantı koptu"),
    }
}

fn refusal_text(r: Refusal) -> NetError {
    err(match r {
        Refusal::BadProtocol => "röle sürümü uyuşmuyor",
        Refusal::NoSuchCode => "kod geçersiz",
        Refusal::RoomFull => "oda dolu",
        Refusal::TooManyAttempts => "çok fazla deneme, biraz bekle",
        Refusal::Busy => "röle meşgul, sonra dene",
    })
}

/// A relay address resolved once; every tunnel of a host reuses it.
#[derive(Clone)]
pub(crate) struct Target {
    host: String,
    addrs: Vec<SocketAddr>,
}

impl Target {
    /// `ToSocketAddrs` has no timeout, so the lookup runs on a helper thread that is simply abandoned on expiry.
    pub(crate) fn resolve(relay: &str, until: Instant) -> Result<Target, NetError> {
        let host = host_part(relay).ok_or_else(|| err("röle adresi geçersiz"))?.to_string();
        let (tx, rx) = mpsc::channel();
        let name = relay.to_string();
        thread::spawn(move || {
            let _ = tx.send(name.to_socket_addrs().map(|i| i.collect::<Vec<_>>()));
        });
        match rx.recv_timeout(until.saturating_duration_since(Instant::now())) {
            Ok(Ok(addrs)) if !addrs.is_empty() => Ok(Target { host, addrs }),
            Ok(_) => Err(err("röle adresi çözülemedi")),
            Err(_) => Err(err("röle yanıt vermedi")),
        }
    }

    pub(crate) fn dial(&self, until: Instant) -> Result<Stream, NetError> {
        let name = ServerName::try_from(self.host.clone()).map_err(|_| err("röle adresi geçersiz"))?;
        let mut sock = None;
        for a in &self.addrs {
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(err("röle yanıt vermedi"));
            }
            if let Ok(s) = TcpStream::connect_timeout(a, left) {
                sock = Some(s);
                break;
            }
        }
        let sock = sock.ok_or_else(|| err("röleye bağlanılamadı"))?;
        let _ = sock.set_nodelay(true);
        let mut conn = ClientConnection::new(tls_config(), name).map_err(|_| err("güvenli bağlantı kurulamadı"))?;
        let mut plain = Stream::Plain(sock);
        while conn.is_handshaking() {
            conn.complete_io(&mut plain.until(until)).map_err(|e| match e.kind() {
                io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock => err("röle yanıt vermedi"),
                _ => err("güvenli bağlantı kurulamadı"),
            })?;
        }
        let Stream::Plain(sock) = plain else { unreachable!("built as Plain above") };
        Ok(Stream::Tls(Box::new(StreamOwned::new(conn, sock))))
    }
}

/// Host part of `host:port`, brackets of an IPv6 literal removed. None when there is no usable port.
pub(crate) fn host_part(addr: &str) -> Option<&str> {
    let (host, port) = addr.rsplit_once(':')?;
    port.parse::<u16>().ok()?;
    let host = host.trim_start_matches('[').trim_end_matches(']');
    (!host.is_empty()).then_some(host)
}

pub(crate) fn send(s: &mut Stream, until: Instant, msg: &ToRelay) -> Result<(), NetError> {
    write_msg(&mut s.until(until), msg).map_err(io_err)
}

/// A joiner or an accepting host waits here: nothing else may be sent before `Paired`.
pub(crate) fn await_paired(s: &mut Stream, until: Instant) -> Result<(), NetError> {
    match read_msg::<FromRelay, _>(&mut s.until(until)).map_err(io_err)? {
        FromRelay::Paired => Ok(()),
        FromRelay::Refused(r) => Err(refusal_text(r)),
        _ => Err(err("röle beklenmeyen yanıt verdi")),
    }
}

pub(crate) fn join(target: &Target, code: Code, until: Instant) -> Result<Stream, NetError> {
    let mut s = target.dial(until)?;
    send(&mut s, until, &ToRelay::Join { protocol: RELAY_PROTOCOL, code })?;
    await_paired(&mut s, until)?;
    Ok(s)
}

/// The host's side of the relay: status, requests for a new code, and ready tunnels.
pub(crate) struct RelayLink {
    status: Arc<Mutex<RelayStatus>>,
    new_code: Sender<()>,
    tunnels: Receiver<Stream>,
    stop: Arc<AtomicBool>,
    ctrl: Arc<Mutex<Option<TcpStream>>>,
}

impl RelayLink {
    pub(crate) fn start(relay: &str, ping_every: Duration) -> RelayLink {
        let status = Arc::new(Mutex::new(RelayStatus::Connecting));
        let (new_code, wants) = mpsc::channel();
        let (tx, tunnels) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let ctrl = Arc::new(Mutex::new(None));
        let shared = Shared { status: status.clone(), stop: stop.clone(), ctrl: ctrl.clone() };
        let relay = relay.to_string();
        thread::spawn(move || {
            let end = control(&relay, ping_every, &shared, &wants, &tx);
            if let Err(e) = end {
                shared.set(RelayStatus::Failed(e.0));
            }
        });
        RelayLink { status, new_code, tunnels, stop, ctrl }
    }

    pub(crate) fn status(&self) -> RelayStatus {
        self.status.lock().unwrap().clone()
    }

    /// Only an open room can change its code; the old one is dead until the relay answers.
    pub(crate) fn request_new_code(&self) {
        let mut st = self.status.lock().unwrap();
        if matches!(*st, RelayStatus::Open(_)) && self.new_code.send(()).is_ok() {
            *st = RelayStatus::Connecting;
        }
    }

    pub(crate) fn try_tunnel(&self) -> Option<Stream> {
        self.tunnels.try_recv().ok()
    }
}

impl Drop for RelayLink {
    /// Closing the control socket at once makes the relay free the room, so the code dies right away.
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(s) = self.ctrl.lock().unwrap().take() {
            let _ = s.shutdown(std::net::Shutdown::Both);
        }
    }
}

struct Shared {
    status: Arc<Mutex<RelayStatus>>,
    stop: Arc<AtomicBool>,
    ctrl: Arc<Mutex<Option<TcpStream>>>,
}

impl Shared {
    /// A thread that lost the race with `relay_close` must not overwrite the final state.
    fn set(&self, s: RelayStatus) {
        let mut g = self.status.lock().unwrap();
        if !self.stop.load(Ordering::SeqCst) {
            *g = s;
        }
    }
}

fn next_frame(buf: &mut Vec<u8>) -> Result<Option<FromRelay>, NetError> {
    let Some(head) = buf.get(..4) else { return Ok(None) };
    let len = u32::from_le_bytes(head.try_into().unwrap()) as usize;
    if len > MAX_RELAY_FRAME {
        return Err(err("röle geçersiz veri gönderdi"));
    }
    if buf.len() < 4 + len {
        return Ok(None);
    }
    let msg = read_msg::<FromRelay, _>(&mut &buf[..4 + len]).map_err(|_| err("röle geçersiz veri gönderdi"))?;
    buf.drain(..4 + len);
    Ok(Some(msg))
}

fn control(
    relay: &str,
    ping_every: Duration,
    shared: &Shared,
    wants: &Receiver<()>,
    tunnels: &Sender<Stream>,
) -> Result<(), NetError> {
    let until = Instant::now() + STEP_WAIT;
    let target = Target::resolve(relay, until)?;
    let mut ctrl = target.dial(until)?;
    *shared.ctrl.lock().unwrap() = ctrl.tcp().try_clone().ok();
    send(&mut ctrl, until, &ToRelay::Open { protocol: RELAY_PROTOCOL })?;
    ctrl.tcp().set_read_timeout(Some(POLL)).map_err(io_err)?;
    ctrl.tcp().set_write_timeout(Some(STEP_WAIT)).map_err(io_err)?;

    let mut buf = Vec::new();
    // the relay answers one request at a time: no Ping or NewCode while an Opened is owed
    let mut owed = Some(Instant::now());
    let mut last_ping = Instant::now();
    let mut chunk = [0u8; 512];
    loop {
        if shared.stop.load(Ordering::SeqCst) {
            return Ok(());
        }
        match owed {
            Some(since) if since.elapsed() > STEP_WAIT => return Err(err("röle yanıt vermedi")),
            Some(_) => {}
            None => {
                if wants.try_recv().is_ok() {
                    ctrl.write_all(&frame(&ToRelay::NewCode)).map_err(io_err)?;
                    owed = Some(Instant::now());
                } else if last_ping.elapsed() >= ping_every {
                    ctrl.write_all(&frame(&ToRelay::Ping)).map_err(io_err)?;
                    last_ping = Instant::now();
                }
            }
        }
        match ctrl.read(&mut chunk) {
            Ok(0) => return Err(err("röle bağlantısı koptu")),
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(e) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut | io::ErrorKind::Interrupted) => {}
            Err(_) => return Err(err("röle bağlantısı koptu")),
        }
        while let Some(msg) = next_frame(&mut buf)? {
            match msg {
                FromRelay::Opened { code } => {
                    owed = None;
                    last_ping = Instant::now();
                    shared.set(RelayStatus::Open(code));
                }
                FromRelay::Incoming { id } => {
                    let (target, tx) = (target.clone(), tunnels.clone());
                    thread::spawn(move || {
                        // a failed tunnel only costs that one joiner, who sees the relay's pairing timeout
                        if let Ok(s) = accept_tunnel(&target, id) {
                            let _ = tx.send(s);
                        }
                    });
                }
                FromRelay::Refused(r) => return Err(refusal_text(r)),
                FromRelay::Paired => {}
            }
        }
    }
}

fn frame(msg: &ToRelay) -> Vec<u8> {
    let mut v = Vec::new();
    write_msg(&mut v, msg).expect("write to a Vec cannot fail");
    v
}

fn accept_tunnel(target: &Target, id: u64) -> Result<Stream, NetError> {
    let until = Instant::now() + STEP_WAIT;
    let mut s = target.dial(until)?;
    send(&mut s, until, &ToRelay::Accept { id })?;
    await_paired(&mut s, until)?;
    Ok(s)
}
