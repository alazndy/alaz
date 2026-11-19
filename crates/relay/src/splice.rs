use crate::limits::Limits;
use crate::Transport;
use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// Both directions share one transport behind a mutex because `Transport` cannot be cloned
/// (a TLS stream has one state). Reads use a short timeout so the lock is released often.
const POLL: Duration = Duration::from_millis(10);
const CHUNK: usize = 16 * 1024;

struct Bucket {
    tokens: f64,
    rate: f64,
    burst: f64,
    last: Instant,
}

impl Bucket {
    fn new(rate: u64, burst: u64) -> Bucket {
        Bucket { tokens: burst as f64, rate: rate.max(1) as f64, burst: burst as f64, last: Instant::now() }
    }

    /// How long to wait before `n` bytes may pass; zero when they may pass now (and are taken).
    fn take(&mut self, n: usize) -> Duration {
        let now = Instant::now();
        self.tokens = (self.tokens + now.duration_since(self.last).as_secs_f64() * self.rate).min(self.burst);
        self.last = now;
        let need = n as f64;
        if self.tokens >= need {
            self.tokens -= need;
            Duration::ZERO
        } else {
            Duration::from_secs_f64((need - self.tokens) / self.rate)
        }
    }
}

type Shared = Arc<Mutex<Box<dyn Transport>>>;

struct Pump {
    done: Arc<AtomicBool>,
    moved: Arc<AtomicU64>,
    cut: Arc<AtomicBool>,
    abuse_cut: u64,
}

fn is_retry(e: &io::Error) -> bool {
    matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut | io::ErrorKind::Interrupted)
}

fn write_all(dst: &Shared, mut data: &[u8], done: &AtomicBool) -> io::Result<()> {
    while !data.is_empty() {
        if done.load(Ordering::Relaxed) {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        let r = dst.lock().unwrap().write(data);
        match r {
            Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
            Ok(n) => data = &data[n..],
            Err(e) if is_retry(&e) => thread::sleep(Duration::from_millis(1)),
            Err(e) => return Err(e),
        }
    }
    dst.lock().unwrap().flush()
}

fn pump(src: Shared, dst: Shared, p: Pump, lim: &Limits) {
    let mut bucket = Bucket::new(lim.tunnel_rate, lim.tunnel_burst);
    let chunk = CHUNK.min(lim.tunnel_burst.max(1) as usize);
    let mut buf = vec![0u8; chunk];
    while !p.done.load(Ordering::Relaxed) {
        let r = src.lock().unwrap().read(&mut buf);
        let n = match r {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if is_retry(&e) => {
                // lets the other direction take the lock
                thread::sleep(Duration::from_millis(1));
                continue;
            }
            Err(_) => break,
        };
        loop {
            let wait = bucket.take(n);
            if wait.is_zero() {
                break;
            }
            if p.done.load(Ordering::Relaxed) {
                return finish(&src, &dst, &p);
            }
            thread::sleep(wait.min(Duration::from_millis(50)));
        }
        if p.moved.fetch_add(n as u64, Ordering::Relaxed) + n as u64 > p.abuse_cut {
            p.cut.store(true, Ordering::Relaxed);
            break;
        }
        if write_all(&dst, &buf[..n], &p.done).is_err() {
            break;
        }
    }
    finish(&src, &dst, &p);
}

fn finish(src: &Shared, dst: &Shared, p: &Pump) {
    p.done.store(true, Ordering::Relaxed);
    src.lock().unwrap().shutdown();
    dst.lock().unwrap().shutdown();
}

/// Copies bytes both ways, unparsed, until either side closes. Returns true when cut for abuse.
pub fn splice(a: Box<dyn Transport>, b: Box<dyn Transport>, lim: &Limits) -> bool {
    a.set_read_timeout(Some(POLL));
    b.set_read_timeout(Some(POLL));
    let a: Shared = Arc::new(Mutex::new(a));
    let b: Shared = Arc::new(Mutex::new(b));
    let done = Arc::new(AtomicBool::new(false));
    let moved = Arc::new(AtomicU64::new(0));
    let cut = Arc::new(AtomicBool::new(false));
    let mk = || Pump { done: done.clone(), moved: moved.clone(), cut: cut.clone(), abuse_cut: lim.abuse_cut };
    let other = {
        let (a, b, p, lim) = (a.clone(), b.clone(), mk(), lim.clone());
        thread::spawn(move || pump(b, a, p, &lim))
    };
    pump(a, b, mk(), lim);
    let _ = other.join();
    cut.load(Ordering::Relaxed)
}
