//! TLS front for the relay: certificate loading with hot reload, and a rustls-backed `Transport`.
use crate::{Relay, Transport};
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::{ServerConfig, ServerConnection, StreamOwned};
use socket2::{SockRef, TcpKeepalive};
use std::io::{self, Read, Write};
use std::net::{IpAddr, Shutdown, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

fn invalid<E: std::fmt::Display>(what: &str, e: E) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("{what}: {e}"))
}

/// Reads a PEM chain and key and builds a server config. Only the `ring` provider is linked.
pub fn acceptor(cert: &Path, key: &Path) -> io::Result<Arc<ServerConfig>> {
    let chain = CertificateDer::pem_file_iter(cert)
        .map_err(|e| invalid("certificate file", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| invalid("certificate file", e))?;
    if chain.is_empty() {
        return Err(invalid("certificate file", "no certificate found"));
    }
    let key = PrivateKeyDer::from_pem_file(key).map_err(|e| invalid("key file", e))?;
    let cfg =
        ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .map_err(|e| invalid("tls versions", e))?
            .with_no_client_auth()
            .with_single_cert(chain, key)
            .map_err(|e| invalid("certificate and key", e))?;
    Ok(Arc::new(cfg))
}

/// The current server config, swappable at run time so a renewed certificate needs no restart.
pub struct Reloadable {
    cert: PathBuf,
    key: PathBuf,
    current: RwLock<Arc<ServerConfig>>,
}

impl Reloadable {
    pub fn new(cert: &Path, key: &Path) -> io::Result<Arc<Reloadable>> {
        let current = RwLock::new(acceptor(cert, key)?);
        Ok(Arc::new(Reloadable {
            cert: cert.to_path_buf(),
            key: key.to_path_buf(),
            current,
        }))
    }

    /// On error the previous config stays in force: a half-written renewal must not take the relay down.
    pub fn reload(&self) -> io::Result<()> {
        let fresh = acceptor(&self.cert, &self.key)?;
        *self.current.write().unwrap() = fresh;
        Ok(())
    }

    pub fn config(&self) -> Arc<ServerConfig> {
        self.current.read().unwrap().clone()
    }

    /// Reloads on every wake-up message and every `every`; after a failed reload it tries again after
    /// `retry` (a renewal may still be half written). Returns when the sender is dropped.
    pub fn watch(self: Arc<Self>, wake: Receiver<()>, every: Duration, retry: Duration) {
        let mut wait = every;
        loop {
            match wake.recv_timeout(wait) {
                Ok(()) | Err(RecvTimeoutError::Timeout) => match self.reload() {
                    Ok(()) => {
                        eprintln!("alaz-relay: certificate reloaded");
                        wait = every;
                    }
                    Err(e) => {
                        eprintln!(
                            "alaz-relay: certificate reload failed, keeping the old one: {e}"
                        );
                        wait = retry;
                    }
                },
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    }
}

/// One accepted TLS connection. The handshake runs lazily inside the first read, under the caller's timeouts.
pub struct TlsConn {
    tls: StreamOwned<ServerConnection, TcpStream>,
    // a second handle to the same socket: the stream is behind `&mut`, but timeouts and shutdown take `&self`
    sock: TcpStream,
    ip: IpAddr,
}

impl TlsConn {
    pub fn new(cfg: Arc<ServerConfig>, sock: TcpStream) -> io::Result<TlsConn> {
        let ip = sock.peer_addr()?.ip().to_canonical();
        // std has no keepalive API; without it a vanished peer (NAT drop, power loss) pins a room for hours
        let keepalive = TcpKeepalive::new()
            .with_time(Duration::from_secs(60))
            .with_interval(Duration::from_secs(15));
        SockRef::from(&sock).set_tcp_keepalive(&keepalive)?;
        sock.set_nodelay(true)?;
        // safety net for callers that forget their own timeouts: a silent peer must not block a thread for good
        sock.set_read_timeout(Some(DEFAULT_TIMEOUT))?;
        sock.set_write_timeout(Some(DEFAULT_TIMEOUT))?;
        let handle = sock.try_clone()?;
        let conn = ServerConnection::new(cfg).map_err(|e| invalid("tls", e))?;
        Ok(TlsConn {
            tls: StreamOwned::new(conn, sock),
            sock: handle,
            ip,
        })
    }
}

impl Read for TlsConn {
    fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
        self.tls.read(b)
    }
}

impl Write for TlsConn {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        // rustls keeps the bytes it accepted even when pushing them to the socket timed out, so a timeout
        // here means nothing was consumed and the caller may retry the same slice. A zero-length accept of
        // a non-empty slice would read as a dead peer to `write_all`; it only means "buffer full, retry".
        match self.tls.write(b) {
            Ok(0) if !b.is_empty() => Err(io::ErrorKind::WouldBlock.into()),
            other => other,
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        self.tls.flush()
    }
}

impl Transport for TlsConn {
    fn peer_ip(&self) -> IpAddr {
        self.ip
    }

    fn shutdown(&self) {
        let _ = self.sock.shutdown(Shutdown::Both);
    }

    fn set_read_timeout(&self, d: Option<Duration>) {
        let _ = self.sock.set_read_timeout(d);
    }

    fn set_write_timeout(&self, d: Option<Duration>) {
        let _ = self.sock.set_write_timeout(d);
    }
}

/// Accepts forever, wrapping every socket in TLS with the config current at that moment.
pub fn accept_loop(listener: TcpListener, relay: Arc<Relay>, certs: Arc<Reloadable>) {
    for incoming in listener.incoming() {
        match incoming {
            Ok(sock) => match TlsConn::new(certs.config(), sock) {
                Ok(conn) => relay.serve(conn),
                Err(_) => continue,
            },
            // out of file descriptors and similar: back off instead of spinning
            Err(_) => thread::sleep(Duration::from_millis(50)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Limits;
    use alaz_relay_proto::{FromRelay, RELAY_PROTOCOL, ToRelay, read_msg, write_msg};
    use rustls::pki_types::ServerName;
    use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};
    use std::io::{Read, Write};
    use std::net::{SocketAddr, TcpListener};
    use std::path::PathBuf;
    use std::thread;
    use std::time::{Duration, Instant};

    struct Pki {
        dir: PathBuf,
        cert: PathBuf,
        key: PathBuf,
    }

    impl Pki {
        fn new(name: &str) -> Pki {
            let dir =
                std::env::temp_dir().join(format!("alaz-relay-{name}-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let p = Pki {
                cert: dir.join("cert.pem"),
                key: dir.join("key.pem"),
                dir,
            };
            p.issue();
            p
        }

        /// Writes a fresh self-signed certificate over the previous one; returns its DER.
        fn issue(&self) -> CertificateDer<'static> {
            let ck = rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
            std::fs::write(&self.cert, ck.cert.pem()).unwrap();
            std::fs::write(&self.key, ck.key_pair.serialize_pem()).unwrap();
            ck.cert.der().clone()
        }
    }

    impl Drop for Pki {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    type Client = StreamOwned<ClientConnection, TcpStream>;

    fn client_config(trusted: &[CertificateDer<'static>]) -> Arc<ClientConfig> {
        let mut roots = RootCertStore::empty();
        for c in trusted {
            roots.add(c.clone()).unwrap();
        }
        let cfg =
            ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_safe_default_protocol_versions()
                .unwrap()
                .with_root_certificates(roots)
                .with_no_client_auth();
        Arc::new(cfg)
    }

    fn dial(addr: SocketAddr, cfg: &Arc<ClientConfig>) -> Client {
        let mut sock = TcpStream::connect(addr).unwrap();
        sock.set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut conn =
            ClientConnection::new(cfg.clone(), ServerName::try_from("localhost").unwrap()).unwrap();
        while conn.is_handshaking() {
            conn.complete_io(&mut sock).unwrap();
        }
        StreamOwned::new(conn, sock)
    }

    fn start(certs: &Arc<Reloadable>) -> (Arc<Relay>, SocketAddr) {
        start_with(certs, Limits::default())
    }

    fn start_with(certs: &Arc<Reloadable>, limits: Limits) -> (Arc<Relay>, SocketAddr) {
        let relay = Relay::new(Limits {
            fail_delay: Duration::ZERO,
            ..limits
        });
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        let (r, c) = (relay.clone(), certs.clone());
        thread::spawn(move || accept_loop(l, r, c));
        (relay, addr)
    }

    #[test]
    fn tls_roundtrip_join_pairs_through_acceptor() {
        let pki = Pki::new("roundtrip");
        let der = CertificateDer::from_pem_file(&pki.cert).unwrap();
        let certs = Reloadable::new(&pki.cert, &pki.key).unwrap();
        let (_relay, addr) = start(&certs);
        let cfg = client_config(&[der]);

        let mut ctrl = dial(addr, &cfg);
        write_msg(
            &mut ctrl,
            &ToRelay::Open {
                protocol: RELAY_PROTOCOL,
            },
        )
        .unwrap();
        let FromRelay::Opened { code } = read_msg(&mut ctrl).unwrap() else {
            panic!("not opened")
        };

        let mut joiner = dial(addr, &cfg);
        write_msg(
            &mut joiner,
            &ToRelay::Join {
                protocol: RELAY_PROTOCOL,
                code,
            },
        )
        .unwrap();
        let FromRelay::Incoming { id } = read_msg(&mut ctrl).unwrap() else {
            panic!("no incoming")
        };
        let mut host = dial(addr, &cfg);
        write_msg(&mut host, &ToRelay::Accept { id }).unwrap();
        assert_eq!(
            read_msg::<FromRelay, _>(&mut host).unwrap(),
            FromRelay::Paired
        );
        assert_eq!(
            read_msg::<FromRelay, _>(&mut joiner).unwrap(),
            FromRelay::Paired
        );

        joiner.write_all(b"hello host").unwrap();
        let mut got = [0u8; 10];
        host.read_exact(&mut got).unwrap();
        assert_eq!(&got, b"hello host");
        host.write_all(b"hello joiner").unwrap();
        let mut got = [0u8; 12];
        joiner.read_exact(&mut got).unwrap();
        assert_eq!(&got, b"hello joiner");
    }

    #[test]
    fn plain_tcp_client_is_dropped() {
        let pki = Pki::new("plain");
        let certs = Reloadable::new(&pki.cert, &pki.key).unwrap();
        let (relay, addr) = start(&certs);
        let mut s = TcpStream::connect(addr).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        write_msg(
            &mut s,
            &ToRelay::Open {
                protocol: RELAY_PROTOCOL,
            },
        )
        .unwrap();
        s.write_all(&[0u8; 64]).unwrap();
        let end = Instant::now() + Duration::from_secs(5);
        let mut buf = [0u8; 256];
        loop {
            match s.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(_) => assert!(Instant::now() < end, "connection stayed open"),
            }
        }
        assert_eq!(relay.counters().rooms, 0);
    }

    #[test]
    fn reload_picks_up_new_certificate() {
        let pki = Pki::new("reload");
        let first = CertificateDer::from_pem_file(&pki.cert).unwrap();
        let certs = Reloadable::new(&pki.cert, &pki.key).unwrap();
        let (_relay, addr) = start(&certs);
        let second = pki.issue();
        assert_ne!(first, second);
        let cfg = client_config(&[first.clone(), second.clone()]);

        let seen = |c: &Client| c.conn.peer_certificates().unwrap()[0].clone().into_owned();
        assert_eq!(
            seen(&dial(addr, &cfg)),
            first,
            "old certificate until reload"
        );
        certs.reload().unwrap();
        assert_eq!(
            seen(&dial(addr, &cfg)),
            second,
            "new certificate after reload"
        );

        std::fs::write(&pki.cert, "not a certificate").unwrap();
        assert!(certs.reload().is_err());
        assert_eq!(
            seen(&dial(addr, &cfg)),
            second,
            "a broken file keeps the working certificate"
        );
    }

    /// True when the peer closed the connection (EOF or reset) within `within`.
    fn closed_within(s: &mut TcpStream, within: Duration) -> bool {
        s.set_read_timeout(Some(within)).unwrap();
        let mut buf = [0u8; 64];
        let end = Instant::now() + within;
        while Instant::now() < end {
            match s.read(&mut buf) {
                Ok(0) => return true,
                Ok(_) => {}
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                    ) =>
                {
                    return false;
                }
                Err(_) => return true,
            }
        }
        false
    }

    #[test]
    fn stalled_handshake_is_cut_at_first_frame_timeout() {
        let pki = Pki::new("stall");
        let certs = Reloadable::new(&pki.cert, &pki.key).unwrap();
        let limits = Limits {
            first_frame_timeout: Duration::from_millis(400),
            ..Limits::default()
        };
        let (_relay, addr) = start_with(&certs, limits);
        let mut silent = TcpStream::connect(addr).unwrap();
        let t = Instant::now();
        assert!(
            closed_within(&mut silent, Duration::from_secs(5)),
            "silent socket stayed open"
        );
        assert!(
            t.elapsed() < Duration::from_secs(3),
            "cut by control_idle (60 s), not the first-frame deadline"
        );
    }

    #[test]
    fn silent_sockets_over_the_per_ip_cap_are_closed() {
        let pki = Pki::new("cap");
        let certs = Reloadable::new(&pki.cert, &pki.key).unwrap();
        let limits = Limits {
            max_conns_per_ip: 2,
            first_frame_timeout: Duration::from_millis(500),
            ..Limits::default()
        };
        let (_relay, addr) = start_with(&certs, limits);
        let mut socks: Vec<TcpStream> = (0..6).map(|_| TcpStream::connect(addr).unwrap()).collect();
        for s in socks.iter_mut() {
            assert!(
                closed_within(s, Duration::from_secs(4)),
                "a silent socket stayed open"
            );
        }
    }

    #[test]
    fn watch_retries_soon_after_a_failed_reload() {
        let pki = Pki::new("retry");
        let certs = Reloadable::new(&pki.cert, &pki.key).unwrap();
        let before = certs.config();
        let good = std::fs::read_to_string(&pki.cert).unwrap();
        std::fs::write(&pki.cert, "half written").unwrap();
        let (wake, woken) = std::sync::mpsc::channel();
        let c = certs.clone();
        thread::spawn(move || {
            c.watch(woken, Duration::from_secs(3600), Duration::from_millis(100))
        });
        wake.send(()).unwrap();
        thread::sleep(Duration::from_millis(250));
        assert!(
            Arc::ptr_eq(&before, &certs.config()),
            "a failed reload must keep the old config"
        );
        pki.issue();
        let end = Instant::now() + Duration::from_secs(5);
        while Arc::ptr_eq(&before, &certs.config()) {
            assert!(Instant::now() < end, "no retry after the failed reload");
            thread::sleep(Duration::from_millis(20));
        }
        let _ = good;
    }

    #[test]
    fn stalled_peer_write_times_out_as_would_block() {
        let pki = Pki::new("wtimeout");
        let der = CertificateDer::from_pem_file(&pki.cert).unwrap();
        let certs = Reloadable::new(&pki.cert, &pki.key).unwrap();
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        let cfg = client_config(&[der]);
        let client = thread::spawn(move || {
            let c = dial(addr, &cfg);
            thread::sleep(Duration::from_secs(3));
            drop(c);
        });
        let (sock, _) = l.accept().unwrap();
        let mut server = TlsConn::new(certs.config(), sock).unwrap();
        // the handshake runs inside the first read; the client sends no data, so this ends in a timeout
        server.set_read_timeout(Some(Duration::from_millis(300)));
        let mut byte = [0u8; 1];
        let first = server.read(&mut byte).unwrap_err();
        assert!(
            matches!(
                first.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
            ),
            "{first:?}"
        );
        server.set_write_timeout(Some(Duration::from_millis(50)));
        let chunk = vec![7u8; 64 * 1024];
        let end = Instant::now() + Duration::from_secs(2);
        let err = loop {
            match server.write(&chunk) {
                Ok(n) => assert!(n > 0 && Instant::now() < end, "write never blocked"),
                Err(e) => break e,
            }
        };
        assert!(
            matches!(
                err.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
            ),
            "{err:?}"
        );
        let _ = client.join();
    }
}
