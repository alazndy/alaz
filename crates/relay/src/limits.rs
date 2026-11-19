use std::time::Duration;

/// Every tunable of the relay. Tests inject short values; defaults follow the design spec.
#[derive(Clone, Debug)]
pub struct Limits {
    /// Failed code attempts allowed per IP inside `fail_window`; one more locks the IP out.
    pub max_fails_per_window: u32,
    pub fail_window: Duration,
    pub lockout: Duration,
    /// Join attempts per second over all IPs.
    pub global_attempts_per_sec: u32,
    pub max_conns_per_ip: usize,
    pub max_rooms: usize,
    /// Players per room, host included.
    pub max_players: usize,
    /// Joiners paired or waiting, over all rooms.
    pub max_tunnels: usize,
    /// Whole-frame deadline for a connection's first message, and the longest a host control
    /// connection may send no frame at all (a `Ping` counts as a frame).
    pub control_idle: Duration,
    /// A tunnel write that makes no progress for this long means the receiver is stuck: the tunnel is closed.
    pub write_stall: Duration,
    pub pair_timeout: Duration,
    /// Failed lookups answer no sooner than this, so a miss costs the same however it came about.
    pub fail_delay: Duration,
    /// Sustained bytes per second per tunnel direction.
    pub tunnel_rate: u64,
    /// Bucket size in bytes: a large snapshot passes without waiting.
    pub tunnel_burst: u64,
    /// A tunnel moving more than this in total is cut and counted; only for extreme abuse.
    pub abuse_cut: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_fails_per_window: 10,
            fail_window: Duration::from_secs(60),
            lockout: Duration::from_secs(300),
            global_attempts_per_sec: 100,
            max_conns_per_ip: 64,
            max_rooms: 1000,
            max_players: 8,
            max_tunnels: 4000,
            control_idle: Duration::from_secs(60),
            write_stall: Duration::from_secs(30),
            pair_timeout: Duration::from_secs(10),
            fail_delay: Duration::from_millis(300),
            tunnel_rate: 2 << 20,
            tunnel_burst: 64 << 20,
            abuse_cut: 20 << 30,
        }
    }
}
