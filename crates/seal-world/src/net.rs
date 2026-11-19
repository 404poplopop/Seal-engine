//! Snapshot replication: the server world is authoritative, clients interpolate.
//!
//! Packet (little endian): magic "SNP1", seq u32, time f32, count u16, then per object
//! id length u8 + id bytes, translation 3 x f32, rotation 3 x f32, hidden u8.

use crate::{Entity, World};

const MAGIC: &[u8; 4] = b"SNP1";

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub id: String,
    pub pos: [f32; 3],
    pub rot: [f32; 3],
    pub hidden: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub seq: u32,
    pub time: f32,
    pub rows: Vec<Row>,
}

impl Snapshot {
    pub fn capture(world: &World, seq: u32, time: f32) -> Snapshot {
        let rows = world
            .entities
            .iter()
            .map(|e| Row {
                id: e.id.clone(),
                pos: e.translation,
                rot: e.rotation,
                hidden: e.hidden,
            })
            .collect();
        Snapshot { seq, time, rows }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut b = MAGIC.to_vec();
        b.extend(self.seq.to_le_bytes());
        b.extend(self.time.to_le_bytes());
        b.extend((self.rows.len() as u16).to_le_bytes());
        for r in &self.rows {
            let id = &r.id.as_bytes()[..r.id.len().min(255)];
            b.push(id.len() as u8);
            b.extend(id);
            for v in r.pos.iter().chain(r.rot.iter()) {
                b.extend(v.to_le_bytes());
            }
            b.push(r.hidden as u8);
        }
        b
    }

    pub fn decode(b: &[u8]) -> Result<Snapshot, String> {
        let mut at = 0usize;
        let mut take = |n: usize| -> Result<&[u8], String> {
            let s = b.get(at..at + n).ok_or("packet too short")?;
            at += n;
            Ok(s)
        };
        if take(4)? != MAGIC {
            return Err("not a snapshot".into());
        }
        let f = |s: &[u8]| f32::from_le_bytes([s[0], s[1], s[2], s[3]]);
        let seq = u32::from_le_bytes(take(4)?.try_into().unwrap());
        let time = f(take(4)?);
        let n = u16::from_le_bytes(take(2)?.try_into().unwrap());
        let mut rows = Vec::new();
        for _ in 0..n {
            let len = take(1)?[0] as usize;
            let id = String::from_utf8(take(len)?.to_vec()).map_err(|_| "bad id")?;
            let mut v = [0.0f32; 6];
            for x in &mut v {
                *x = f(take(4)?);
            }
            let hidden = take(1)?[0] != 0;
            rows.push(Row { id, pos: [v[0], v[1], v[2]], rot: [v[3], v[4], v[5]], hidden });
        }
        Ok(Snapshot { seq, time, rows })
    }
}

/// Client side: keeps the newest snapshots and shows the world slightly in the past.
#[derive(Default)]
pub struct Client {
    pub(crate) snaps: Vec<Snapshot>,
    pub last_seq: Option<u32>,
}

impl Client {
    /// Take a packet; stale or repeated sequence numbers are ignored. Returns whether it was used.
    pub fn receive(&mut self, bytes: &[u8]) -> bool {
        let Ok(s) = Snapshot::decode(bytes) else { return false };
        if self.last_seq.is_some_and(|l| s.seq <= l) {
            return false;
        }
        self.last_seq = Some(s.seq);
        self.snaps.push(s);
        if self.snaps.len() > 8 {
            self.snaps.remove(0);
        }
        true
    }

    /// Put `world`'s objects where the server had them at server time `t`
    /// (blend between the two snapshots around it; hold the newest/oldest outside).
    pub fn apply(&self, world: &mut World, t: f32) {
        let Some(newest) = self.snaps.last() else { return };
        let after = self.snaps.iter().position(|s| s.time >= t);
        let (a, b, u) = match after {
            None => (newest, newest, 0.0),
            Some(0) => (&self.snaps[0], &self.snaps[0], 0.0),
            Some(i) => {
                let (a, b) = (&self.snaps[i - 1], &self.snaps[i]);
                (a, b, ((t - a.time) / (b.time - a.time)).clamp(0.0, 1.0))
            }
        };
        for rb in &b.rows {
            let Some(e) = world.entities.iter_mut().find(|e| e.id == rb.id) else { continue };
            let ra = a.rows.iter().find(|r| r.id == rb.id).unwrap_or(rb);
            Self::set(e, ra, rb, u);
        }
    }

    fn set(e: &mut Entity, a: &Row, b: &Row, u: f32) {
        for i in 0..3 {
            e.translation[i] = a.pos[i] + (b.pos[i] - a.pos[i]) * u;
            e.rotation[i] = a.rot[i] + (b.rot[i] - a.rot[i]) * u;
        }
        e.hidden = b.hidden;
    }
}

// ---- Play over the network: a host world runs the game, joined worlds show it. ----

const INPUT_MAGIC: &[u8; 4] = b"INP1";

/// What a joined player sends each frame: held buttons and key presses since the last packet.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InputMsg {
    pub seq: u32,
    pub input: crate::PlayInput,
    pub keys: Vec<(String, bool)>,
}

impl InputMsg {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = INPUT_MAGIC.to_vec();
        b.extend(self.seq.to_le_bytes());
        b.push(
            self.input.left as u8 | (self.input.right as u8) << 1 | (self.input.jump as u8) << 2,
        );
        let keys = &self.keys[..self.keys.len().min(255)];
        b.push(keys.len() as u8);
        for (name, pressed) in keys {
            let n = &name.as_bytes()[..name.len().min(255)];
            b.push(n.len() as u8);
            b.extend(n);
            b.push(*pressed as u8);
        }
        b
    }

    pub fn decode(b: &[u8]) -> Result<InputMsg, String> {
        let mut at = 0usize;
        let mut take = |n: usize| -> Result<&[u8], String> {
            let s = b.get(at..at + n).ok_or("packet too short")?;
            at += n;
            Ok(s)
        };
        if take(4)? != INPUT_MAGIC {
            return Err("not an input packet".into());
        }
        let seq = u32::from_le_bytes(take(4)?.try_into().unwrap());
        let flags = take(1)?[0];
        let input =
            crate::PlayInput { left: flags & 1 != 0, right: flags & 2 != 0, jump: flags & 4 != 0 };
        let n = take(1)?[0];
        let mut keys = Vec::new();
        for _ in 0..n {
            let len = take(1)?[0] as usize;
            let name = String::from_utf8(take(len)?.to_vec()).map_err(|_| "bad key name")?;
            keys.push((name, take(1)?[0] != 0));
        }
        Ok(InputMsg { seq, input, keys })
    }
}

/// The authoritative side: takes joined players' input and sends a snapshot every frame.
pub struct Host {
    socket: std::net::UdpSocket,
    peers: Vec<(std::net::SocketAddr, u32, crate::PlayInput)>,
    seq: u32,
}

impl Host {
    /// Listen on `port` on every network interface (0 picks a free port).
    pub fn bind(port: u16) -> std::io::Result<Host> {
        Self::bind_on("0.0.0.0", port)
    }

    /// Listen on one interface only, such as `127.0.0.1` for players on this computer.
    pub fn bind_on(ip: &str, port: u16) -> std::io::Result<Host> {
        let socket = std::net::UdpSocket::bind((ip, port))?;
        socket.set_nonblocking(true)?;
        Ok(Host { socket, peers: Vec::new(), seq: 0 })
    }

    pub fn port(&self) -> u16 {
        self.socket.local_addr().map(|a| a.port()).unwrap_or(0)
    }

    pub fn peer_count(&self) -> usize {
        self.peers.len()
    }

    /// Read waiting input: new players are added; each one's held buttons join the host's own.
    pub fn read_input(&mut self, world: &mut World) {
        let mut buf = [0u8; 2048];
        while let Ok((n, from)) = self.socket.recv_from(&mut buf) {
            let Ok(msg) = InputMsg::decode(&buf[..n]) else { continue };
            let slot = match self.peers.iter().position(|p| p.0 == from) {
                Some(i) => i,
                None => {
                    world.play_log.push(format!("net: player joined from {from}"));
                    self.peers.push((from, 0, crate::PlayInput::default()));
                    self.peers.len() - 1
                }
            };
            if msg.seq < self.peers[slot].1 {
                continue;
            }
            self.peers[slot].1 = msg.seq;
            self.peers[slot].2 = msg.input;
            world.play_keys.extend(msg.keys);
        }
        for (_, _, input) in &self.peers {
            world.play_input.left |= input.left;
            world.play_input.right |= input.right;
            world.play_input.jump |= input.jump;
        }
    }

    /// Send the world as it is now to every joined player.
    pub fn send_snapshot(&mut self, world: &World) {
        self.seq += 1;
        let bytes = Snapshot::capture(world, self.seq, world.play_time).encode();
        for (addr, _, _) in &self.peers {
            let _ = self.socket.send_to(&bytes, addr);
        }
    }
}

/// A joined player: sends input, shows the host's world a little in the past.
pub struct Join {
    socket: std::net::UdpSocket,
    client: Client,
    seq: u32,
    newest: f32,
    /// Seconds behind the newest snapshot that the world is shown, so there are two to blend.
    pub delay: f32,
}

impl Join {
    pub fn connect(addr: &str) -> std::io::Result<Join> {
        let target: std::net::SocketAddr =
            std::net::ToSocketAddrs::to_socket_addrs(addr)?.next().ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidInput, "no address")
            })?;
        let local = if target.ip().is_loopback() { "127.0.0.1" } else { "0.0.0.0" };
        let socket = std::net::UdpSocket::bind((local, 0))?;
        socket.connect(target)?;
        socket.set_nonblocking(true)?;
        Ok(Join { socket, client: Client::default(), seq: 0, newest: 0.0, delay: 0.1 })
    }

    /// Whether any snapshot has arrived yet.
    pub fn connected(&self) -> bool {
        self.client.last_seq.is_some()
    }

    /// Send this frame's input, take every waiting snapshot, and pose the world.
    pub fn frame(&mut self, world: &mut World, dt: f32) {
        self.seq += 1;
        let msg = InputMsg {
            seq: self.seq,
            input: world.play_input,
            keys: std::mem::take(&mut world.play_keys),
        };
        let _ = self.socket.send(&msg.encode());
        let mut buf = [0u8; 65536];
        while let Ok(n) = self.socket.recv(&mut buf) {
            if self.client.receive(&buf[..n]) {
                if let Some(s) = self.client.snaps.last() {
                    self.newest = self.newest.max(s.time);
                }
            }
        }
        let _ = dt;
        self.client.apply(world, self.newest - self.delay);
    }
}

/// How this world takes part in network play.
pub enum NetRole {
    Host(Host),
    Join(Join),
}
