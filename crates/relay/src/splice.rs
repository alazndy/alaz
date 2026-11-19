use crate::Transport;
use crate::limits::Limits;
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
        Bucket {
            tokens: burst as f64,
            rate: rate.max(1) as f64,
            burst: burst as f64,
            last: Instant::now(),
        }
    }

    /// How long to wait before `n` bytes may pass; zero when they may pass now (and are taken).
    fn take(&mut self, n: usize) -> Duration {
        let now = Instant::now();
        self.tokens =
            (self.tokens + now.duration_since(self.last).as_secs_f64() * self.rate).min(self.burst);
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

/// Millisecond clock of the last byte seen in either direction.
struct Activity {
    start: Instant,
    last_ms: AtomicU64,
}

impl Activity {
    fn touch(&self) {
        self.last_ms
            .store(self.start.elapsed().as_millis() as u64, Ordering::Relaxed);
    }

    fn idle_for(&self) -> Duration {
        let now = self.start.elapsed().as_millis() as u64;
        Duration::from_millis(now.saturating_sub(self.last_ms.load(Ordering::Relaxed)))
    }
}

struct Pump {
    stall: Duration,
    idle: Duration,
    activity: Arc<Activity>,
    done: Arc<AtomicBool>,
    moved: Arc<AtomicU64>,
    cut: Arc<AtomicBool>,
    abuse_cut: u64,
}

fn is_retry(e: &io::Error) -> bool {
    matches!(
        e.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut | io::ErrorKind::Interrupted
    )
}

/// Retries on timeout but never holds the lock across a wait, so teardown can always take it.
fn write_all(dst: &Shared, mut data: &[u8], done: &AtomicBool, stall: Duration) -> io::Result<()> {
    let mut progress = Instant::now();
    while !data.is_empty() {
        if done.load(Ordering::Relaxed) {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        let r = dst.lock().unwrap().write(data);
        match r {
            Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
            Ok(n) => {
                data = &data[n..];
                progress = Instant::now();
            }
            Err(e) if is_retry(&e) => {
                if progress.elapsed() > stall {
                    return Err(io::ErrorKind::TimedOut.into());
                }
                thread::sleep(Duration::from_millis(1));
            }
            Err(e) => return Err(e),
        }
    }
    loop {
        let r = dst.lock().unwrap().flush();
        match r {
            Err(e)
                if is_retry(&e) && !done.load(Ordering::Relaxed) && progress.elapsed() <= stall =>
            {
                thread::sleep(Duration::from_millis(1));
            }
            other => return other,
        }
    }
}

fn pump(src: Shared, dst: Shared, p: Pump, lim: &Limits) {
    let mut bucket = Bucket::new(lim.tunnel_rate, lim.tunnel_burst);
    let chunk = CHUNK.min(lim.tunnel_burst.max(1) as usize);
    let mut buf = vec![0u8; chunk];
    while !p.done.load(Ordering::Relaxed) {
        let r = src.lock().unwrap().read(&mut buf);
        let n = match r {
            Ok(0) => break,
            Ok(n) => {
                p.activity.touch();
                n
            }
            Err(e) if is_retry(&e) => {
                // lets the other direction take the lock
                thread::sleep(Duration::from_millis(1));
                if p.activity.idle_for() > p.idle {
                    break;
                }
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
        if write_all(&dst, &buf[..n], &p.done, p.stall).is_err() {
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
    a.set_write_timeout(Some(POLL));
    b.set_write_timeout(Some(POLL));
    let a: Shared = Arc::new(Mutex::new(a));
    let b: Shared = Arc::new(Mutex::new(b));
    let done = Arc::new(AtomicBool::new(false));
    let moved = Arc::new(AtomicU64::new(0));
    let cut = Arc::new(AtomicBool::new(false));
    let activity = Arc::new(Activity {
        start: Instant::now(),
        last_ms: AtomicU64::new(0),
    });
    let mk = || Pump {
        stall: lim.write_stall,
        idle: lim.tunnel_idle,
        activity: activity.clone(),
        done: done.clone(),
        moved: moved.clone(),
        cut: cut.clone(),
        abuse_cut: lim.abuse_cut,
    };
    let other = {
        let (a, b, p, lim) = (a.clone(), b.clone(), mk(), lim.clone());
        crate::spawn_small(lim.stack_size, move || pump(b, a, p, &lim))
    };
    let Ok(other) = other else {
        finish(&a, &b, &mk());
        return false;
    };
    pump(a, b, mk(), lim);
    let _ = other.join();
    cut.load(Ordering::Relaxed)
}
