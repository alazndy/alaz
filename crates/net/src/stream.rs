//! One byte pipe for `Conn`: a plain LAN socket or a TLS session to the relay.
use rustls::{ClientConnection, StreamOwned};
use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::time::Instant;

/// Detects a dead path (relay VM gone, NAT dropped the mapping) in about a minute instead of the OS default of
/// hours. Probes only fire on an idle socket; a quiet but healthy host answers them, so no app-level timeout is needed.
/// Failure to set it is not fatal: the connection just keeps the OS defaults.
pub(crate) fn set_keepalive(sock: &TcpStream) {
    let ka = socket2::TcpKeepalive::new()
        .with_time(std::time::Duration::from_secs(30))
        .with_interval(std::time::Duration::from_secs(10));
    #[cfg(target_os = "linux")]
    let ka = ka.with_retries(3);
    let _ = socket2::SockRef::from(sock).set_tcp_keepalive(&ka);
}

pub(crate) enum Stream {
    Plain(TcpStream),
    Tls(Box<StreamOwned<ClientConnection, TcpStream>>),
}

impl Stream {
    pub(crate) fn tcp(&self) -> &TcpStream {
        match self {
            Stream::Plain(s) => s,
            Stream::Tls(t) => &t.sock,
        }
    }

    /// A `Read + Write` view whose every operation is cut off at `until`, however slowly the peer trickles.
    pub(crate) fn until(&mut self, until: Instant) -> Bounded<'_> {
        Bounded { stream: self, until }
    }
}

impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Stream::Plain(s) => s.read(buf),
            Stream::Tls(t) => t.read(buf),
        }
    }
}

impl Write for Stream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Stream::Plain(s) => s.write(buf),
            Stream::Tls(t) => t.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Stream::Plain(s) => s.flush(),
            Stream::Tls(t) => t.flush(),
        }
    }
}

pub(crate) struct Bounded<'a> {
    stream: &'a mut Stream,
    until: Instant,
}

impl Bounded<'_> {
    fn arm(&self) -> io::Result<()> {
        let left = self.until.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(io::ErrorKind::TimedOut.into());
        }
        let t = self.stream.tcp();
        t.set_read_timeout(Some(left))?;
        t.set_write_timeout(Some(left))
    }
}

impl Read for Bounded<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.arm()?;
        self.stream.read(buf)
    }
}

impl Write for Bounded<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.arm()?;
        self.stream.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.arm()?;
        self.stream.flush()
    }
}

