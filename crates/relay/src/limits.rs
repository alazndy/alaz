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
    /// Part of `global_attempts_per_sec` that IPs with a recent failed guess may not use, so a guesser
    /// cannot starve addresses that have behaved.
    pub reserved_attempts_per_sec: u32,
    /// Sockets alive at once over everything; checked before a thread is started.
    pub max_total_conns: usize,
    /// Per IPv4 address or IPv6 /64 (a /48 may use four times as many).
    pub max_conns_per_ip: usize,
    /// Sockets a locked-out IP may hold. It gets a small pool instead of being shut out, because a
    /// host behind the same NAT as a guesser must still be able to Open and Accept.
    pub max_locked_per_ip: usize,
    /// Locked-IP sockets over all IPs; beyond it they are closed unanswered.
    pub max_locked_total: usize,
    /// Per IPv4 address or IPv6 /48.
    pub max_rooms_per_ip: usize,
    /// Joiners (paired or waiting) per IPv4 address or IPv6 /48.
    pub max_tunnels_per_ip: usize,
    pub max_rooms: usize,
    /// Players per room, host included.
    pub max_players: usize,
    /// Joiners paired or waiting, over all rooms.
    pub max_tunnels: usize,
    /// Whole-frame deadline for a connection's first message, and the longest a host control
    /// connection may send no frame at all (a `Ping` counts as a frame).
    pub control_idle: Duration,
    /// Deadline for a connection's first whole frame, TLS handshake included, and for refusing an
    /// over-limit connection: an idle socket must not hold a thread for the full control idle time.
    pub first_frame_timeout: Duration,
    /// A tunnel write that makes no progress for this long means the receiver is stuck: the tunnel is closed.
    pub write_stall: Duration,
    /// A paired tunnel that moves no byte in either direction for this long is closed. Long on purpose:
    /// a host whose window is minimised or hidden may stop ticking, and must not lose its players.
    pub tunnel_idle: Duration,
    /// Stack of every thread the relay starts; small because a connection thread does little.
    pub stack_size: usize,
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
            global_attempts_per_sec: 20,
            reserved_attempts_per_sec: 10,
            max_total_conns: 3000,
            max_conns_per_ip: 64,
            max_locked_per_ip: 8,
            max_locked_total: 256,
            max_rooms_per_ip: 4,
            max_tunnels_per_ip: 16,
            max_rooms: 500,
            max_players: 8,
            max_tunnels: 1000,
            control_idle: Duration::from_secs(60),
            first_frame_timeout: Duration::from_secs(10),
            write_stall: Duration::from_secs(30),
            tunnel_idle: Duration::from_secs(300),
            stack_size: 128 << 10,
            pair_timeout: Duration::from_secs(10),
            fail_delay: Duration::from_millis(300),
            tunnel_rate: 2 << 20,
            tunnel_burst: 64 << 20,
            abuse_cut: 20 << 30,
        }
    }
}
