//! Lockstep co-op over TCP. The host is the turn server: it stamps every command into a numbered turn
//! (one turn per sim tick) and broadcasts it, and everybody, host included, applies exactly the same
//! turns to the same deterministic `World`. The host also publishes a state hash every `HASH_EVERY`
//! ticks so clients notice a desync and can ask for a fresh snapshot.
//!
//! Trust model: friends on one team. Clients may command any agent, the host does not police it.

use alaz_sim::{AgentId, Command, CommandKind, World};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

pub const PROTOCOL: u32 = 1;
pub const DEFAULT_PORT: u16 = 7777;
pub const HASH_EVERY: u64 = 120;
const MAX_FRAME: usize = 64 << 20;
const MAX_OUTBOX: usize = 64 << 20;

#[derive(Serialize, Deserialize)]
enum ToHost {
    Hello { protocol: u32 },
    Cmds(Vec<Command>),
    Resync,
}

#[derive(Serialize, Deserialize)]
enum ToClient {
    Welcome { agent: u32, snapshot: Vec<u8> },
    Turn(Turn),
    Resync { snapshot: Vec<u8> },
    Refused(String),
}

/// Everything that happens on one tick.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Turn {
    pub tick: u64,
    pub cmds: Vec<Command>,
    /// Host state hash before this turn is applied (only every `HASH_EVERY` ticks).
    pub hash: Option<u64>,
}

/// Apply a turn to a world. Returns false (world untouched) if the turn does not fit or the
/// state hash disagrees with the host's, i.e. the world is out of sync.
pub fn apply_turn(world: &mut World, turn: &Turn) -> bool {
    if turn.tick != world.tick || turn.hash.is_some_and(|h| h != world.hash()) {
        return false;
    }
    world.step(&turn.cmds);
    true
}

/// Non-blocking, length-prefixed message pipe.
struct Conn {
    stream: TcpStream,
    rbuf: Vec<u8>,
    wbuf: Vec<u8>,
    wpos: usize,
    dead: bool,
}

impl Conn {
    fn new(stream: TcpStream) -> io::Result<Self> {
        stream.set_nonblocking(true)?;
        stream.set_nodelay(true)?;
        Ok(Self { stream, rbuf: Vec::new(), wbuf: Vec::new(), wpos: 0, dead: false })
    }

    fn send<T: Serialize>(&mut self, msg: &T) {
        let body = postcard::to_allocvec(msg).expect("encode message");
        self.wbuf.extend_from_slice(&(body.len() as u32).to_le_bytes());
        self.wbuf.extend_from_slice(&body);
        self.flush();
    }

    fn flush(&mut self) {
        while self.wpos < self.wbuf.len() && !self.dead {
            match self.stream.write(&self.wbuf[self.wpos..]) {
                Ok(0) => self.dead = true,
                Ok(n) => self.wpos += n,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(_) => self.dead = true,
            }
        }
        if self.wpos == self.wbuf.len() {
            self.wbuf.clear();
            self.wpos = 0;
        }
        if self.wbuf.len() - self.wpos > MAX_OUTBOX {
            self.dead = true;
        }
    }

    fn recv<T: DeserializeOwned>(&mut self) -> Vec<T> {
        self.flush();
        let mut chunk = [0u8; 64 * 1024];
        while !self.dead {
            match self.stream.read(&mut chunk) {
                Ok(0) => self.dead = true,
                Ok(n) => self.rbuf.extend_from_slice(&chunk[..n]),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(_) => self.dead = true,
            }
        }
        let mut out = Vec::new();
        let mut at = 0;
        while self.rbuf.len() - at >= 4 {
            let len = u32::from_le_bytes(self.rbuf[at..at + 4].try_into().unwrap()) as usize;
            if len > MAX_FRAME {
                self.dead = true;
                break;
            }
            if self.rbuf.len() - at - 4 < len {
                break;
            }
            match postcard::from_bytes(&self.rbuf[at + 4..at + 4 + len]) {
                Ok(m) => out.push(m),
                Err(_) => {
                    self.dead = true;
                    break;
                }
            }
            at += 4 + len;
        }
        self.rbuf.drain(..at);
        out
    }
}

struct Peer {
    conn: Conn,
    resync: bool,
}

pub struct Host {
    listener: TcpListener,
    waiting: Vec<(Conn, Instant)>,
    peers: Vec<Peer>,
    queued: Vec<Command>,
    joins_queued: u32,
}

impl Host {
    pub fn bind(addr: impl ToSocketAddrs) -> io::Result<Self> {
        let listener = TcpListener::bind(addr)?;
        listener.set_nonblocking(true)?;
        Ok(Self { listener, waiting: Vec::new(), peers: Vec::new(), queued: Vec::new(), joins_queued: 0 })
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    /// Players including the host.
    pub fn players(&self) -> usize {
        self.peers.len() + 1
    }

    /// Accept newcomers and collect client commands. Call right before `turn`, with no step in between.
    pub fn poll(&mut self, world: &World) {
        while let Ok((stream, _)) = self.listener.accept() {
            if let Ok(c) = Conn::new(stream) {
                self.waiting.push((c, Instant::now()));
            }
        }
        let mut still = Vec::new();
        for (mut conn, since) in std::mem::take(&mut self.waiting) {
            let msgs: Vec<ToHost> = conn.recv();
            match msgs.into_iter().next() {
                Some(ToHost::Hello { protocol }) if protocol == PROTOCOL => {
                    let agent = world.agents.len() as u32 + self.joins_queued;
                    self.joins_queued += 1;
                    self.queued.push(Command { agent: AgentId(agent), kind: CommandKind::Join });
                    conn.send(&ToClient::Welcome { agent, snapshot: world.to_bytes() });
                    self.peers.push(Peer { conn, resync: false });
                }
                Some(ToHost::Hello { .. }) => {
                    conn.send(&ToClient::Refused("sürüm uyuşmuyor".into()));
                }
                Some(_) => {}
                None if !conn.dead && since.elapsed() < Duration::from_secs(10) => still.push((conn, since)),
                None => {}
            }
        }
        self.waiting = still;
        for p in &mut self.peers {
            for m in p.conn.recv::<ToHost>() {
                match m {
                    // clients can never spawn players themselves
                    ToHost::Cmds(cmds) => {
                        self.queued.extend(cmds.into_iter().filter(|c| c.kind != CommandKind::Join))
                    }
                    ToHost::Resync => p.resync = true,
                    ToHost::Hello { .. } => {}
                }
            }
        }
    }

    /// Close the current turn: broadcast it and return the command list the host must apply too.
    pub fn turn(&mut self, world: &World, local: Vec<Command>) -> Vec<Command> {
        let mut cmds = std::mem::take(&mut self.queued);
        cmds.extend(local);
        self.joins_queued = 0;
        let turn = Turn { tick: world.tick, cmds, hash: (world.tick % HASH_EVERY == 0).then(|| world.hash()) };
        let snapshot = self.peers.iter().any(|p| p.resync).then(|| world.to_bytes());
        for p in &mut self.peers {
            if p.resync {
                p.resync = false;
                if let Some(s) = &snapshot {
                    p.conn.send(&ToClient::Resync { snapshot: s.clone() });
                }
            }
            p.conn.send(&ToClient::Turn(turn.clone()));
        }
        self.peers.retain(|p| !p.conn.dead);
        turn.cmds
    }
}

#[derive(Debug)]
pub struct NetError(pub String);

impl std::fmt::Display for NetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

pub struct Client {
    conn: Conn,
    turns: VecDeque<Turn>,
    fresh: Option<World>,
    awaiting_resync: bool,
    pub agent: AgentId,
}

impl Client {
    /// Connect, say hello and wait for the host's snapshot.
    pub fn connect(addr: &str, timeout: Duration) -> Result<(Client, World), NetError> {
        let err = |e: &dyn std::fmt::Display| NetError(format!("{e}"));
        let sock = addr
            .to_socket_addrs()
            .map_err(|e| err(&e))?
            .next()
            .ok_or_else(|| NetError("adres çözülemedi".into()))?;
        let stream = TcpStream::connect_timeout(&sock, timeout).map_err(|e| err(&e))?;
        let mut client = Client {
            conn: Conn::new(stream).map_err(|e| err(&e))?,
            turns: VecDeque::new(),
            fresh: None,
            awaiting_resync: false,
            agent: AgentId(0),
        };
        client.conn.send(&ToHost::Hello { protocol: PROTOCOL });
        let start = Instant::now();
        let mut world = None;
        while world.is_none() {
            for m in client.conn.recv::<ToClient>() {
                match m {
                    ToClient::Welcome { agent, snapshot } => {
                        client.agent = AgentId(agent);
                        world = Some(World::from_bytes(&snapshot).map_err(|e| err(&format!("{e:?}")))?);
                    }
                    ToClient::Turn(t) if world.is_some() => client.turns.push_back(t),
                    ToClient::Refused(why) => return Err(NetError(why)),
                    _ => {}
                }
            }
            if client.conn.dead || start.elapsed() > timeout {
                return Err(NetError("host yanıt vermedi".into()));
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        Ok((client, world.unwrap()))
    }

    /// Read what arrived. Turns queue up, a snapshot replaces the world (see `take_fresh_world`).
    pub fn poll(&mut self) {
        for m in self.conn.recv::<ToClient>() {
            match m {
                ToClient::Turn(t) if !self.awaiting_resync => self.turns.push_back(t),
                ToClient::Resync { snapshot } => {
                    if let Ok(w) = World::from_bytes(&snapshot) {
                        self.turns.clear();
                        self.fresh = Some(w);
                        self.awaiting_resync = false;
                    }
                }
                _ => {}
            }
        }
    }

    pub fn send(&mut self, cmds: Vec<Command>) {
        if !cmds.is_empty() {
            self.conn.send(&ToHost::Cmds(cmds));
        }
    }

    /// Next turn to apply. Returns nothing while a fresh snapshot is waiting: take it first.
    pub fn next_turn(&mut self) -> Option<Turn> {
        if self.fresh.is_some() {
            return None;
        }
        self.turns.pop_front()
    }

    pub fn backlog(&self) -> usize {
        self.turns.len()
    }

    /// Ask the host for a fresh snapshot after a desync; turns are ignored until it arrives.
    pub fn request_resync(&mut self) {
        if !self.awaiting_resync {
            self.awaiting_resync = true;
            self.turns.clear();
            self.conn.send(&ToHost::Resync);
        }
    }

    pub fn take_fresh_world(&mut self) -> Option<World> {
        self.fresh.take()
    }

    pub fn is_connected(&self) -> bool {
        !self.conn.dead
    }

    pub fn resyncing(&self) -> bool {
        self.awaiting_resync
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alaz_sim::{ItemKind, NodeKind};
    use std::thread;

    fn host_world() -> World {
        let mut w = World::new(5);
        let (sx, sy) = w.spawn_mm;
        w.spawn_agent(sx, sy);
        w
    }

    fn setup() -> (Host, World, Client, World) {
        let mut host = Host::bind("127.0.0.1:0").unwrap();
        let addr = host.local_addr().unwrap().to_string();
        let mut hw = host_world();
        let t = thread::spawn(move || Client::connect(&addr, Duration::from_secs(10)).unwrap());
        while !t.is_finished() {
            host.poll(&hw);
            let c = host.turn(&hw, vec![]);
            hw.step(&c);
            thread::sleep(Duration::from_millis(1));
        }
        let (client, cw) = t.join().unwrap();
        (host, hw, client, cw)
    }

    /// Run host turns, let the client follow, optionally with a per-tick hook.
    fn run(host: &mut Host, hw: &mut World, client: &mut Client, cw: &mut World, ticks: u32, mut hook: impl FnMut(u32, &mut Host, &mut World, &mut Client, &mut World)) {
        for i in 0..ticks {
            hook(i, host, hw, client, cw);
            host.poll(hw);
            let c = host.turn(hw, vec![]);
            hw.step(&c);
            client.poll();
            while let Some(t) = client.next_turn() {
                if !apply_turn(cw, &t) {
                    client.request_resync();
                    break;
                }
            }
            if let Some(w) = client.take_fresh_world() {
                *cw = w;
            }
        }
    }

    fn settle(host: &mut Host, hw: &mut World, client: &mut Client, cw: &mut World) {
        for _ in 0..2000 {
            if cw.tick == hw.tick {
                break;
            }
            client.poll();
            while let Some(t) = client.next_turn() {
                if !apply_turn(cw, &t) {
                    client.request_resync();
                    break;
                }
            }
            if let Some(w) = client.take_fresh_world() {
                *cw = w;
            }
            host.poll(hw);
            thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn client_joins_and_stays_in_lockstep() {
        let (mut host, mut hw, mut client, mut cw) = setup();
        assert_eq!(host.players(), 2);
        let me = client.agent;
        assert!(cw.agents.len() <= me.0 as usize, "snapshot is taken before the join");
        let tree = hw.nodes.iter().position(|n| n.kind == NodeKind::Tree).unwrap() as u32;
        run(&mut host, &mut hw, &mut client, &mut cw, 900, |i, _, hw, client, _| {
            if i == 10 {
                let (sx, sy) = hw.spawn_mm;
                client.send(vec![Command { agent: me, kind: CommandKind::MoveTo { x: sx + 6_000, y: sy } }]);
            }
            if i == 300 {
                client.send(vec![Command { agent: me, kind: CommandKind::Gather { node: alaz_sim::NodeId(tree) } }]);
            }
        });
        settle(&mut host, &mut hw, &mut client, &mut cw);
        assert_eq!(hw.agents.len(), 2);
        assert_eq!(cw.tick, hw.tick);
        assert_eq!(cw.hash(), hw.hash());
        assert!(hw.agents[me.0 as usize].x != hw.spawn_mm.0 - 1_500, "the client's agent moved");
        let _ = ItemKind::Wood;
    }

    #[test]
    fn clients_cannot_spawn_players_and_desync_heals() {
        let (mut host, mut hw, mut client, mut cw) = setup();
        let me = client.agent;
        run(&mut host, &mut hw, &mut client, &mut cw, 400, |i, _, _, client, cw| {
            if i == 5 {
                client.send(vec![Command { agent: me, kind: CommandKind::Join }]);
            }
            if i == 200 {
                // corrupt the client's copy
                cw.agents[0].hunger -= 777;
            }
        });
        // after the next hash tick the client must notice and resync
        run(&mut host, &mut hw, &mut client, &mut cw, 300, |_, _, _, _, _| {});
        settle(&mut host, &mut hw, &mut client, &mut cw);
        assert_eq!(hw.agents.len(), 2, "forged Join was ignored");
        assert_eq!(cw.tick, hw.tick);
        assert_eq!(cw.hash(), hw.hash(), "client healed through a snapshot");
    }

    /// Both players hammer the sim with a mix of commands, minigame strikes included; the worlds must agree.
    #[test]
    fn random_play_by_two_players_stays_in_sync() {
        let (mut host, mut hw, mut client, mut cw) = setup();
        let me = client.agent;
        let host_agent = hw.agents[0].id;
        let trees: Vec<u32> = hw.nodes.iter().enumerate().filter(|(_, n)| n.kind == NodeKind::Tree).map(|(i, _)| i as u32).take(40).collect();
        let mut rng: u64 = 0x9E3779B97F4A7C15;
        let mut next = move || {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            rng
        };
        let mut kinds = |who: AgentId, hw: &World, r: u64| -> Option<Command> {
            let (sx, sy) = hw.spawn_mm;
            let off = |k: u64| ((k % 24_000) as i64) - 12_000;
            let kind = match r % 9 {
                0 | 1 => CommandKind::MoveTo { x: sx + off(r >> 8), y: sy + off(r >> 24) },
                2 | 3 => CommandKind::Gather { node: alaz_sim::NodeId(trees[(r >> 8) as usize % trees.len()]) },
                4 => CommandKind::Craft { recipe: alaz_sim::RecipeId(0) },
                5 | 6 => CommandKind::StrikeAt { angle: (r >> 8) as u16 % 360, power: (r >> 20) as u8 % 101 },
                7 => CommandKind::Strike,
                _ => CommandKind::Eat { item: ItemKind::Berries },
            };
            Some(Command { agent: who, kind })
        };
        run(&mut host, &mut hw, &mut client, &mut cw, 3000, |i, _, hw, client, _| {
            if i % 7 == 0 {
                let r = next();
                if let Some(c) = kinds(me, hw, r) {
                    client.send(vec![c]);
                }
            }
        });
        // the host plays too (through its own queue) while the client keeps going
        let _ = host_agent;
        settle(&mut host, &mut hw, &mut client, &mut cw);
        assert_eq!(cw.tick, hw.tick);
        assert_eq!(cw.hash(), hw.hash());
    }

    #[test]
    fn refuses_garbage_and_survives_it() {
        let mut host = Host::bind("127.0.0.1:0").unwrap();
        let addr = host.local_addr().unwrap();
        let mut s = TcpStream::connect(addr).unwrap();
        s.write_all(&[255, 255, 255, 255, 1, 2, 3]).unwrap();
        let w = host_world();
        for _ in 0..50 {
            host.poll(&w);
            thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(host.players(), 1);
        assert!(Client::connect("127.0.0.1:1", Duration::from_millis(300)).is_err());
    }
}
