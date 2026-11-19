//! `alaz-relay --listen ADDR --cert PATH --key PATH [--stats-addr LOOPBACK_ADDR]`
use alaz_relay::tls::{self, Reloadable};
use alaz_relay::{Limits, Relay};
use std::io::Write;
use std::net::{SocketAddr, TcpListener};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::Duration;

const RELOAD_EVERY: Duration = Duration::from_secs(12 * 60 * 60);
const RELOAD_RETRY: Duration = Duration::from_secs(60);
const USAGE: &str =
    "usage: alaz-relay --listen ADDR:PORT --cert PEM --key PEM [--stats-addr 127.0.0.1:PORT]";

struct Args {
    listen: SocketAddr,
    cert: PathBuf,
    key: PathBuf,
    stats: Option<SocketAddr>,
}

fn parse_args(args: impl Iterator<Item = String>) -> Result<Args, String> {
    let (mut listen, mut cert, mut key, mut stats) = (None, None, None, None);
    let mut args = args;
    while let Some(flag) = args.next() {
        let value = args.next().ok_or_else(|| format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--listen" => {
                listen = Some(
                    value
                        .parse::<SocketAddr>()
                        .map_err(|e| format!("--listen: {e}"))?,
                )
            }
            "--cert" => cert = Some(PathBuf::from(value)),
            "--key" => key = Some(PathBuf::from(value)),
            "--stats-addr" => {
                let a = value
                    .parse::<SocketAddr>()
                    .map_err(|e| format!("--stats-addr: {e}"))?;
                // an IPv4-mapped IPv6 address is not "loopback" to std; rejecting it is the safe reading
                if !a.ip().is_loopback() {
                    return Err("--stats-addr must be a loopback address".into());
                }
                stats = Some(a);
            }
            other => return Err(format!("unknown argument {other}")),
        }
    }
    Ok(Args {
        listen: listen.ok_or("--listen is required")?,
        cert: cert.ok_or("--cert is required")?,
        key: key.ok_or("--key is required")?,
        stats,
    })
}

/// Plain text, one `name value` pair per line; the request is ignored, so `nc` and `curl --http0.9` both work.
fn serve_stats(listener: TcpListener, relay: Arc<Relay>) {
    for conn in listener.incoming().flatten() {
        let c = relay.counters();
        let text = format!(
            "rooms {}\ntunnels {}\nrefusals {}\nlockouts {}\nabuse_cuts {}\n",
            c.rooms, c.tunnels, c.refusals, c.lockouts, c.abuse_cuts
        );
        let mut conn = conn;
        let _ = conn.set_write_timeout(Some(Duration::from_secs(2)));
        let _ = conn.write_all(text.as_bytes());
    }
}

fn run() -> Result<(), String> {
    let args = parse_args(std::env::args().skip(1)).map_err(|e| format!("{e}\n{USAGE}"))?;
    let certs = Reloadable::new(&args.cert, &args.key)
        .map_err(|e| format!("cannot load certificate: {e}"))?;
    let listener = TcpListener::bind(args.listen)
        .map_err(|e| format!("cannot listen on {}: {e}", args.listen))?;
    let relay = Relay::new(Limits::default());

    if let Some(addr) = args.stats {
        let l = TcpListener::bind(addr).map_err(|e| format!("cannot bind stats on {addr}: {e}"))?;
        let r = relay.clone();
        thread::spawn(move || serve_stats(l, r));
    }

    let (wake, woken) = mpsc::channel();
    #[cfg(unix)]
    {
        let mut signals = signal_hook::iterator::Signals::new([signal_hook::consts::SIGHUP])
            .map_err(|e| format!("signals: {e}"))?;
        let wake = wake.clone();
        thread::spawn(move || {
            for _ in signals.forever() {
                if wake.send(()).is_err() {
                    return;
                }
            }
        });
    }
    let watched = certs.clone();
    // `wake` stays alive in this frame, which never returns while the accept loop runs
    thread::spawn(move || watched.watch(woken, RELOAD_EVERY, RELOAD_RETRY));

    eprintln!("alaz-relay: listening on {}", args.listen);
    tls::accept_loop(listener, relay, certs);
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("alaz-relay: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::parse_args;

    fn parse(stats: &str) -> Result<super::Args, String> {
        let a = [
            "--listen",
            "0.0.0.0:443",
            "--cert",
            "c",
            "--key",
            "k",
            "--stats-addr",
            stats,
        ];
        parse_args(a.iter().map(|s| s.to_string()))
    }

    #[test]
    fn stats_addr_accepts_loopback_only() {
        assert!(parse("127.0.0.1:9100").is_ok());
        assert!(parse("[::1]:9100").is_ok());
        assert!(parse("0.0.0.0:9100").is_err());
        assert!(parse("192.168.1.5:9100").is_err());
        assert!(parse("[::]:9100").is_err());
        assert!(parse("[::ffff:127.0.0.1]:9100").is_err());
    }
}
