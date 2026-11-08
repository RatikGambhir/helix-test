//! The RuneCraft server: owns the world, streams chunks, relays players, runs mobs and TNT, and
//! saves edits to disk. Runs as its own binary or inside the client for single player and LAN.

use runecraft_common::block::{self, BlockId};
use runecraft_common::chunk::{self, CHUNK, Column, HEIGHT};
use runecraft_common::city::CITIES;
use runecraft_common::net::{encode_frame, read_frame};
use runecraft_common::noise::{hash, unit};
use runecraft_common::physics::{self, Body};
use runecraft_common::protocol::*;
use runecraft_common::worldgen;
use runecraft_common::{DAY_LENGTH, PROTOCOL_VERSION};
use glam::Vec3;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::io::{self, BufWriter, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct Config {
    pub bind: String,
    pub world_dir: PathBuf,
    /// Used only when the world is new.
    pub seed: u64,
    pub motd: String,
    pub max_view: u8,
    pub mobs: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            bind: format!("0.0.0.0:{}", runecraft_common::DEFAULT_PORT),
            world_dir: PathBuf::from("worlds/world"),
            seed: 0x5EED,
            motd: "Welcome to RuneCraft!".into(),
            max_view: 12,
            mobs: true,
        }
    }
}

/// A running server. Dropping it stops the server and saves the world.
pub struct Server {
    pub addr: SocketAddr,
    stop: Arc<AtomicBool>,
    main: Option<thread::JoinHandle<()>>,
}

impl Server {
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // Wake the accept loop.
        let _ = TcpStream::connect_timeout(&self.addr, Duration::from_millis(200));
        if let Some(t) = self.main.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop();
    }
}

enum Event {
    Join { conn: u32, name: String, skin: u8, view: u8, out: Sender<Arc<Vec<u8>>> },
    Msg(u32, ToServer),
    Leave(u32),
}

pub fn start(cfg: Config) -> io::Result<Server> {
    let listener = TcpListener::bind(&cfg.bind)?;
    let mut addr = listener.local_addr()?;
    if addr.ip().is_unspecified() {
        addr.set_ip([127, 0, 0, 1].into());
    }
    let stop = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::channel::<Event>();
    {
        let stop = stop.clone();
        let tx = tx.clone();
        thread::Builder::new().name("accept".into()).spawn(move || accept_loop(listener, tx, stop))?;
    }
    let main = {
        let stop = stop.clone();
        thread::Builder::new().name("world".into()).spawn(move || {
            let mut w = World::open(cfg);
            w.run(rx, stop);
        })?
    };
    Ok(Server { addr, stop, main: Some(main) })
}

fn accept_loop(listener: TcpListener, tx: Sender<Event>, stop: Arc<AtomicBool>) {
    let mut next = 1u32;
    for stream in listener.incoming() {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        let Ok(stream) = stream else { continue };
        let conn = next;
        next += 1;
        let tx = tx.clone();
        let _ = thread::Builder::new().name(format!("conn-{conn}")).spawn(move || serve_conn(conn, stream, tx));
    }
}

fn serve_conn(conn: u32, stream: TcpStream, tx: Sender<Event>) {
    let _ = stream.set_nodelay(true);
    let Ok(write_half) = stream.try_clone() else { return };
    let mut read_half = io::BufReader::new(stream);
    let Ok(ToServer::Hello { name, version, view_distance, skin }) = read_frame::<ToServer, _>(&mut read_half) else { return };
    let (out_tx, out_rx) = mpsc::channel::<Arc<Vec<u8>>>();
    thread::spawn(move || write_loop(write_half, out_rx));
    if version != PROTOCOL_VERSION {
        let _ = out_tx.send(Arc::new(encode_frame(&ToClient::Kicked(format!("Version mismatch: server {PROTOCOL_VERSION}, you {version}")))));
        thread::sleep(Duration::from_millis(300));
        return;
    }
    if tx.send(Event::Join { conn, name, skin, view: view_distance, out: out_tx }).is_err() {
        return;
    }
    while let Ok(msg) = read_frame::<ToServer, _>(&mut read_half) {
        if tx.send(Event::Msg(conn, msg)).is_err() {
            return;
        }
    }
    let _ = tx.send(Event::Leave(conn));
}

fn write_loop(stream: TcpStream, rx: Receiver<Arc<Vec<u8>>>) {
    let mut w = BufWriter::with_capacity(64 << 10, stream);
    while let Ok(first) = rx.recv() {
        if w.write_all(&first).is_err() {
            return;
        }
        while let Ok(more) = rx.try_recv() {
            if w.write_all(&more).is_err() {
                return;
            }
        }
        if w.flush().is_err() {
            return;
        }
    }
    let _ = w.get_ref().shutdown(std::net::Shutdown::Both);
}

#[derive(Serialize, Deserialize, Default)]
struct Save {
    seed: u64,
    time: f32,
    edits: Vec<((i32, i32), Vec<(u32, u8)>)>,
    players: Vec<(String, [f32; 3])>,
}

struct Player {
    name: String,
    skin: u8,
    pose: Pose,
    out: Sender<Arc<Vec<u8>>>,
    view: u8,
    sent: HashSet<(i32, i32)>,
    hurt_cooldown: f32,
}

impl Player {
    fn send(&self, msg: &ToClient) {
        let _ = self.out.send(Arc::new(encode_frame(msg)));
    }
    fn chunk(&self) -> (i32, i32) {
        ((self.pose.pos[0].floor() as i32).div_euclid(CHUNK), (self.pose.pos[2].floor() as i32).div_euclid(CHUNK))
    }
    fn pos(&self) -> Vec3 {
        Vec3::from(self.pose.pos)
    }
}

struct Loaded {
    col: Column,
    encoded: Option<Arc<Vec<u8>>>,
}

struct Mob {
    kind: MobKind,
    body: Body,
    yaw: f32,
    health: f32,
    wander: Vec3,
    timer: f32,
    hurt: f32,
    attack_cd: f32,
}

struct World {
    cfg: Config,
    seed: u64,
    time: f32,
    columns: HashMap<(i32, i32), Loaded>,
    edits: HashMap<(i32, i32), HashMap<u32, u8>>,
    saved_pos: HashMap<String, [f32; 3]>,
    pending: HashSet<(i32, i32)>,
    jobs: Sender<(i32, i32)>,
    done: Receiver<(i32, i32, Column)>,
    players: HashMap<u32, Player>,
    mobs: HashMap<u32, Mob>,
    next_mob: u32,
    fuses: Vec<(f32, [i32; 3])>,
    dirty: bool,
    rng: u64,
    spawn: [f32; 3],
}

impl World {
    fn open(cfg: Config) -> World {
        let save: Save = std::fs::read(cfg.world_dir.join("world.dat"))
            .ok()
            .and_then(|b| bincode::deserialize(&b).ok())
            .unwrap_or(Save { seed: cfg.seed, time: 0.3, ..Default::default() });
        let seed = save.seed;
        let edits = save.edits.into_iter().map(|(k, v)| (k, v.into_iter().collect())).collect();
        let (jobs, job_rx) = mpsc::channel::<(i32, i32)>();
        let (done_tx, done) = mpsc::channel();
        let job_rx = Arc::new(Mutex::new(job_rx));
        let workers = thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(2, 8) - 1;
        for i in 0..workers.max(1) {
            let job_rx = job_rx.clone();
            let done_tx = done_tx.clone();
            let _ = thread::Builder::new().name(format!("gen-{i}")).spawn(move || {
                loop {
                    let job = job_rx.lock().unwrap().recv();
                    let Ok((cx, cz)) = job else { return };
                    let col = worldgen::generate(seed, cx, cz);
                    if done_tx.send((cx, cz, col)).is_err() {
                        return;
                    }
                }
            });
        }
        let rng = hash(seed ^ 0xA11CE);
        World {
            seed,
            time: save.time,
            columns: HashMap::new(),
            edits,
            saved_pos: save.players.into_iter().collect(),
            pending: HashSet::new(),
            jobs,
            done,
            players: HashMap::new(),
            mobs: HashMap::new(),
            next_mob: 1,
            fuses: Vec::new(),
            dirty: false,
            rng,
            spawn: worldgen::find_spawn(seed),
            cfg,
        }
    }

    fn rand(&mut self) -> f32 {
        self.rng = hash(self.rng.wrapping_add(0x9E3779B97F4A7C15));
        unit(self.rng)
    }

    fn save(&mut self) {
        let mut save = Save { seed: self.seed, time: self.time, ..Default::default() };
        save.edits = self.edits.iter().map(|(k, v)| (*k, v.iter().map(|(a, b)| (*a, *b)).collect())).collect();
        for p in self.players.values() {
            self.saved_pos.insert(p.name.clone(), p.pose.pos);
        }
        save.players = self.saved_pos.iter().map(|(k, v)| (k.clone(), *v)).collect();
        let _ = std::fs::create_dir_all(&self.cfg.world_dir);
        if let Ok(bytes) = bincode::serialize(&save) {
            let tmp = self.cfg.world_dir.join("world.dat.tmp");
            if std::fs::write(&tmp, bytes).is_ok() {
                let _ = std::fs::rename(&tmp, self.cfg.world_dir.join("world.dat"));
            }
        }
        self.dirty = false;
    }

    fn run(&mut self, rx: Receiver<Event>, stop: Arc<AtomicBool>) {
        let tick = Duration::from_millis(50);
        let mut last = Instant::now();
        let mut since_save = 0.0f32;
        let mut since_time = 0.0f32;
        let mut since_mobs = 0.0f32;
        let mut since_gc = 0.0f32;
        while !stop.load(Ordering::SeqCst) {
            let deadline = last + tick;
            loop {
                let now = Instant::now();
                if now >= deadline {
                    break;
                }
                match rx.recv_timeout(deadline - now) {
                    Ok(ev) => self.handle(ev),
                    Err(mpsc::RecvTimeoutError::Timeout) => break,
                    Err(mpsc::RecvTimeoutError::Disconnected) => return,
                }
            }
            let now = Instant::now();
            let dt = (now - last).as_secs_f32().min(0.5);
            last = now;
            while let Ok((cx, cz, mut col)) = self.done.try_recv() {
                self.pending.remove(&(cx, cz));
                if let Some(e) = self.edits.get(&(cx, cz)) {
                    for (&i, &b) in e {
                        if (i as usize) < col.blocks.len() {
                            col.blocks[i as usize] = b;
                        }
                    }
                    col.recompute_heights();
                }
                self.columns.insert((cx, cz), Loaded { col, encoded: None });
            }
            self.stream_chunks();
            self.time = (self.time + dt / DAY_LENGTH).fract();
            since_time += dt;
            if since_time > 5.0 {
                since_time = 0.0;
                self.broadcast(&ToClient::Time(self.time));
            }
            self.tick_fuses(dt);
            if self.cfg.mobs {
                self.tick_mobs(dt);
                since_mobs += dt;
                if since_mobs > 2.0 {
                    since_mobs = 0.0;
                    self.spawn_mobs();
                }
            }
            since_gc += dt;
            if since_gc > 10.0 {
                since_gc = 0.0;
                self.unload_far();
            }
            since_save += dt;
            if since_save > 30.0 {
                since_save = 0.0;
                if self.dirty {
                    self.save();
                }
            }
        }
        for p in self.players.values() {
            p.send(&ToClient::Kicked("Server closed".into()));
        }
        self.save();
    }

    fn broadcast(&self, msg: &ToClient) {
        let frame = Arc::new(encode_frame(msg));
        for p in self.players.values() {
            let _ = p.out.send(frame.clone());
        }
    }

    fn broadcast_except(&self, except: u32, msg: &ToClient) {
        let frame = Arc::new(encode_frame(msg));
        for (id, p) in &self.players {
            if *id != except {
                let _ = p.out.send(frame.clone());
            }
        }
    }

    fn system(&self, text: impl Into<String>) {
        self.broadcast(&ToClient::Chat { from: None, text: text.into() });
    }

    fn handle(&mut self, ev: Event) {
        match ev {
            Event::Join { conn, name, skin, view, out } => {
                let mut name: String = name.trim().chars().filter(|c| !c.is_control()).take(16).collect();
                if name.is_empty() {
                    name = format!("Player{conn}");
                }
                let base = name.clone();
                let mut n = 2;
                while self.players.values().any(|p| p.name == name) {
                    name = format!("{base}{n}");
                    n += 1;
                }
                let pos = self.saved_pos.get(&name).copied().unwrap_or(self.spawn);
                let pose = Pose { pos, yaw: 0.0, pitch: 0.0 };
                let p = Player { name: name.clone(), skin, pose, out, view: view.clamp(2, self.cfg.max_view), sent: HashSet::new(), hurt_cooldown: 0.0 };
                p.send(&ToClient::Welcome {
                    id: conn,
                    seed: self.seed,
                    spawn: pos,
                    time: self.time,
                    world: self.cfg.world_dir.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
                    motd: self.cfg.motd.clone(),
                });
                for (id, other) in &self.players {
                    p.send(&ToClient::PlayerJoined { id: *id, name: other.name.clone(), skin: other.skin, pose: other.pose });
                }
                self.broadcast(&ToClient::PlayerJoined { id: conn, name: name.clone(), skin, pose });
                self.players.insert(conn, p);
                self.system(format!("{name} joined the game"));
            }
            Event::Leave(conn) => {
                if let Some(p) = self.players.remove(&conn) {
                    self.saved_pos.insert(p.name.clone(), p.pose.pos);
                    self.dirty = true;
                    self.broadcast(&ToClient::PlayerLeft { id: conn });
                    self.system(format!("{} left the game", p.name));
                }
            }
            Event::Msg(conn, msg) => self.message(conn, msg),
        }
    }

    fn message(&mut self, conn: u32, msg: ToServer) {
        match msg {
            ToServer::Hello { .. } => {}
            ToServer::Move(pose) => {
                if let Some(p) = self.players.get_mut(&conn) {
                    p.pose = pose;
                }
                self.broadcast_except(conn, &ToClient::PlayerMoved { id: conn, pose });
            }
            ToServer::SetBlock { pos, block } => {
                if (0..HEIGHT).contains(&pos[1]) && (block as usize) < block::BLOCK_COUNT {
                    self.set_block(pos, block);
                }
            }
            ToServer::Chat(text) => {
                let text: String = text.chars().filter(|c| !c.is_control()).take(256).collect();
                if text.starts_with('/') {
                    self.command(conn, &text);
                } else if !text.trim().is_empty() {
                    let from = self.players.get(&conn).map(|p| p.name.clone());
                    self.broadcast(&ToClient::Chat { from, text });
                }
            }
            ToServer::Ignite { pos } => {
                if self.get(pos[0], pos[1], pos[2]) == block::TNT {
                    self.set_block(pos, block::AIR);
                    self.fuses.push((3.0, pos));
                }
            }
            ToServer::Attack { mob, damage } => {
                let attacker = self.players.get(&conn).map(|p| p.pos());
                if let (Some(m), Some(from)) = (self.mobs.get_mut(&mob), attacker) {
                    m.health -= damage.clamp(0.0, 10.0);
                    m.hurt = 0.4;
                    let away = (m.body.pos - from).with_y(0.0).normalize_or_zero();
                    m.body.vel += away * 7.0 + Vec3::Y * 5.0;
                    if m.health <= 0.0 {
                        self.mobs.remove(&mob);
                    }
                }
            }
            ToServer::ViewDistance(v) => {
                let max = self.cfg.max_view;
                if let Some(p) = self.players.get_mut(&conn) {
                    p.view = v.clamp(2, max);
                }
            }
            ToServer::Travel { pos } => {
                if let Some(p) = self.players.get_mut(&conn) {
                    p.pose.pos = pos;
                    p.send(&ToClient::Teleport { pos });
                }
            }
        }
    }

    fn command(&mut self, conn: u32, text: &str) {
        let parts: Vec<&str> = text[1..].split_whitespace().collect();
        let reply = |w: &World, t: String| {
            if let Some(p) = w.players.get(&conn) {
                p.send(&ToClient::Chat { from: None, text: t });
            }
        };
        match parts.as_slice() {
            ["help"] => reply(self, "Commands: /tp <city|x y z|player>, /time <day|night|noon|midnight>, /list, /seed, /spawn".into()),
            ["list"] => {
                let names: Vec<_> = self.players.values().map(|p| p.name.clone()).collect();
                reply(self, format!("{} online: {}", names.len(), names.join(", ")));
            }
            ["seed"] => reply(self, format!("Seed: {}", self.seed)),
            ["spawn"] => {
                let pos = self.spawn;
                self.teleport(conn, pos);
            }
            ["time", t] => {
                self.time = match *t {
                    "day" => 0.3,
                    "noon" => 0.5,
                    "night" => 0.8,
                    "midnight" => 0.0,
                    other => other.parse::<f32>().unwrap_or(self.time).fract(),
                };
                self.broadcast(&ToClient::Time(self.time));
                self.system(format!("Time set to {t}"));
            }
            ["tp", x, y, z] => {
                if let (Ok(x), Ok(y), Ok(z)) = (x.parse::<f32>(), y.parse::<f32>(), z.parse::<f32>()) {
                    self.teleport(conn, [x, y, z]);
                }
            }
            ["tp", rest @ ..] if !rest.is_empty() => {
                let want = rest.join(" ").to_lowercase();
                if let Some(c) = CITIES.iter().find(|c| c.name.to_lowercase().replace(' ', "") == want.replace(' ', "")) {
                    let (x, y, z) = c.arrival();
                    self.teleport(conn, [x, y, z]);
                    reply(self, format!("Welcome to {}!", c.name));
                } else if let Some(p) = self.players.values().find(|p| p.name.to_lowercase() == want) {
                    let pos = p.pose.pos;
                    self.teleport(conn, pos);
                } else {
                    reply(self, format!("No city or player called {want}"));
                }
            }
            _ => reply(self, format!("Unknown command: {text}. Try /help")),
        }
    }

    fn teleport(&mut self, conn: u32, pos: [f32; 3]) {
        if let Some(p) = self.players.get_mut(&conn) {
            p.pose.pos = pos;
            p.send(&ToClient::Teleport { pos });
        }
    }

    fn get(&self, x: i32, y: i32, z: i32) -> BlockId {
        let ((cx, lx), (cz, lz)) = (chunk::split(x), chunk::split(z));
        self.columns.get(&(cx, cz)).map_or(block::STONE, |l| l.col.get(lx, y, lz))
    }

    fn set_block(&mut self, pos: [i32; 3], b: BlockId) {
        let [x, y, z] = pos;
        let ((cx, lx), (cz, lz)) = (chunk::split(x), chunk::split(z));
        let Some(l) = self.columns.get_mut(&(cx, cz)) else { return };
        if l.col.get(lx, y, lz) == b {
            return;
        }
        l.col.set(lx, y, lz, b);
        l.encoded = None;
        self.edits.entry((cx, cz)).or_default().insert(chunk::index(lx, y, lz) as u32, b);
        self.dirty = true;
        let frame = Arc::new(encode_frame(&ToClient::BlockChanged { pos, block: b }));
        for p in self.players.values() {
            if p.sent.contains(&(cx, cz)) {
                let _ = p.out.send(frame.clone());
            }
        }
    }

    fn stream_chunks(&mut self) {
        let mut wanted: Vec<(i32, i32)> = Vec::new();
        let ids: Vec<u32> = self.players.keys().copied().collect();
        for id in ids {
            let (center, view) = {
                let p = &self.players[&id];
                (p.chunk(), p.view as i32)
            };
            // Nearest first, up to a budget per tick.
            let mut ring: Vec<(i32, i32, i32)> = Vec::new();
            for dz in -view..=view {
                for dx in -view..=view {
                    let d2 = dx * dx + dz * dz;
                    if d2 <= view * view + 1 {
                        ring.push((d2, center.0 + dx, center.1 + dz));
                    }
                }
            }
            ring.sort_unstable();
            let mut budget = 12;
            for (_, cx, cz) in ring {
                if budget == 0 {
                    break;
                }
                if self.players[&id].sent.contains(&(cx, cz)) {
                    continue;
                }
                match self.columns.get_mut(&(cx, cz)) {
                    Some(l) => {
                        let frame = l.encoded.get_or_insert_with(|| Arc::new(encode_frame(&ToClient::Column { cx, cz, data: l.col.encode() }))).clone();
                        let p = self.players.get_mut(&id).unwrap();
                        let _ = p.out.send(frame);
                        p.sent.insert((cx, cz));
                        budget -= 1;
                    }
                    None => wanted.push((cx, cz)),
                }
            }
            // Forget columns far outside the view.
            let p = self.players.get_mut(&id).unwrap();
            let far: Vec<(i32, i32)> = p
                .sent
                .iter()
                .copied()
                .filter(|&(cx, cz)| (cx - center.0).abs().max((cz - center.1).abs()) > view + 3)
                .collect();
            for (cx, cz) in far {
                p.sent.remove(&(cx, cz));
                p.send(&ToClient::Unload { cx, cz });
            }
        }
        for k in wanted {
            if self.pending.len() > 64 {
                break;
            }
            if self.pending.insert(k) {
                let _ = self.jobs.send(k);
            }
        }
    }

    fn unload_far(&mut self) {
        let centers: Vec<((i32, i32), i32)> = self.players.values().map(|p| (p.chunk(), p.view as i32 + 4)).collect();
        self.columns.retain(|&(cx, cz), _| centers.iter().any(|&((x, z), r)| (cx - x).abs().max((cz - z).abs()) <= r));
    }

    fn tick_fuses(&mut self, dt: f32) {
        for f in &mut self.fuses {
            f.0 -= dt;
        }
        let due: Vec<[i32; 3]> = self.fuses.iter().filter(|f| f.0 <= 0.0).map(|f| f.1).collect();
        self.fuses.retain(|f| f.0 > 0.0);
        for pos in due {
            self.explode(pos);
        }
    }

    fn explode(&mut self, pos: [i32; 3]) {
        let r = 3.6f32;
        let ri = r.ceil() as i32;
        let centre = Vec3::new(pos[0] as f32 + 0.5, pos[1] as f32 + 0.5, pos[2] as f32 + 0.5);
        for dy in -ri..=ri {
            for dz in -ri..=ri {
                for dx in -ri..=ri {
                    let d = ((dx * dx + dy * dy + dz * dz) as f32).sqrt();
                    let jitter = self.rand() * 0.8;
                    if d > r - jitter {
                        continue;
                    }
                    let p = [pos[0] + dx, pos[1] + dy, pos[2] + dz];
                    let b = self.get(p[0], p[1], p[2]);
                    match b {
                        block::AIR | block::BEDROCK | block::OBSIDIAN | block::WATER | block::LAVA => {}
                        block::TNT => {
                            self.set_block(p, block::AIR);
                            let fuse = 0.4 + self.rand() * 0.6;
                            self.fuses.push((fuse, p));
                        }
                        _ => self.set_block(p, block::AIR),
                    }
                }
            }
        }
        self.broadcast(&ToClient::Explosion { pos: centre.to_array() });
        for p in self.players.values() {
            let d = p.pos().distance(centre);
            if d < 8.0 {
                p.send(&ToClient::Damage { amount: (8.0 - d) * 2.5, from: centre.to_array() });
            }
        }
        self.mobs.retain(|_, m| m.body.pos.distance(centre) > 4.0);
    }

    fn mob_get(&self) -> impl Fn(i32, i32, i32) -> BlockId + '_ {
        |x, y, z| self.get(x, y, z)
    }

    fn is_night(&self) -> bool {
        !(0.23..0.77).contains(&self.time)
    }

    fn spawn_mobs(&mut self) {
        let positions: Vec<Vec3> = self.players.values().map(|p| p.pos()).collect();
        let night = self.is_night();
        for at in positions {
            let near = self.mobs.values().filter(|m| m.body.pos.distance(at) < 64.0).count();
            if near >= 10 {
                continue;
            }
            let ang = self.rand() * std::f32::consts::TAU;
            let dist = 20.0 + self.rand() * 28.0;
            let x = (at.x + ang.cos() * dist).floor() as i32;
            let z = (at.z + ang.sin() * dist).floor() as i32;
            let ((cx, lx), (cz, lz)) = (chunk::split(x), chunk::split(z));
            let Some(l) = self.columns.get(&(cx, cz)) else { continue };
            let y = l.col.height(lx, lz);
            if y < 1 {
                continue;
            }
            let ground = l.col.get(lx, y, lz);
            let kind = if night {
                MobKind::Zombie
            } else if ground == block::GRASS {
                if self.rand() < 0.5 { MobKind::Pig } else { MobKind::Sheep }
            } else {
                continue;
            };
            if !matches!(runecraft_common::city::zone(x, z), runecraft_common::city::Zone::None) && kind != MobKind::Zombie {
                continue;
            }
            let id = self.next_mob;
            self.next_mob += 1;
            let health = if kind == MobKind::Zombie { 20.0 } else { 10.0 };
            let body = Body::new(Vec3::new(x as f32 + 0.5, y as f32 + 1.0, z as f32 + 0.5));
            self.mobs.insert(id, Mob { kind, body, yaw: ang, health, wander: Vec3::ZERO, timer: 0.0, hurt: 0.0, attack_cd: 0.0 });
        }
    }

    fn tick_mobs(&mut self, dt: f32) {
        let players: Vec<(u32, Vec3)> = self.players.iter().map(|(id, p)| (*id, p.pos())).collect();
        let day = !self.is_night();
        let ids: Vec<u32> = self.mobs.keys().copied().collect();
        let mut hits: Vec<(u32, Vec3)> = Vec::new();
        let mut gone = Vec::new();
        for id in ids {
            let mut m = self.mobs.remove(&id).unwrap();
            let nearest = players.iter().map(|(pid, p)| (*pid, *p, p.distance(m.body.pos))).min_by(|a, b| a.2.total_cmp(&b.2));
            let far = nearest.is_none_or(|n| n.2 > 96.0);
            let loaded = self.columns.contains_key(&(
                (m.body.pos.x.floor() as i32).div_euclid(CHUNK),
                (m.body.pos.z.floor() as i32).div_euclid(CHUNK),
            ));
            if far || !loaded || (m.kind == MobKind::Zombie && day && self.rand() < dt * 0.1) || m.body.pos.y < -10.0 {
                gone.push(id);
                continue;
            }
            m.timer -= dt;
            m.hurt = (m.hurt - dt).max(0.0);
            m.attack_cd = (m.attack_cd - dt).max(0.0);
            let mut wish = m.wander;
            if m.kind == MobKind::Zombie
                && let Some((pid, ppos, d)) = nearest
                && d < 28.0
            {
                let dir = (ppos - m.body.pos).with_y(0.0).normalize_or_zero();
                wish = dir * 3.2;
                if d < 1.4 && (ppos.y - m.body.pos.y).abs() < 1.6 && m.attack_cd == 0.0 {
                    m.attack_cd = 1.0;
                    hits.push((pid, m.body.pos));
                }
            } else if m.timer <= 0.0 {
                m.timer = 2.0 + self.rand() * 4.0;
                let a = self.rand() * std::f32::consts::TAU;
                m.wander = if self.rand() < 0.4 { Vec3::ZERO } else { Vec3::new(a.cos(), 0.0, a.sin()) * 1.6 };
                wish = m.wander;
            }
            if wish.length_squared() > 0.01 {
                let target = wish.z.atan2(wish.x);
                m.yaw = target;
            }
            let before = m.body.pos;
            let jump = {
                // Hop up a block when walking into one.
                let ahead = m.body.pos + wish.normalize_or_zero() * 0.6;
                let g = self.mob_get();
                wish.length_squared() > 0.01
                    && m.body.on_ground
                    && block::is_solid(g(ahead.x.floor() as i32, m.body.pos.y.floor() as i32, ahead.z.floor() as i32))
            };
            let input = physics::Input { wish, jump, descend: false, flying: false };
            {
                let g = self.mob_get();
                m.body.step(dt, &input, &g);
            }
            let _ = before;
            self.mobs.insert(id, m);
        }
        for id in gone {
            self.mobs.remove(&id);
        }
        for (pid, from) in hits {
            if let Some(p) = self.players.get_mut(&pid)
                && p.hurt_cooldown <= 0.0
            {
                p.hurt_cooldown = 0.5;
                p.send(&ToClient::Damage { amount: 3.0, from: from.to_array() });
            }
        }
        for p in self.players.values_mut() {
            p.hurt_cooldown -= dt;
        }
        for p in self.players.values() {
            let at = p.pos();
            let list: Vec<MobState> = self
                .mobs
                .iter()
                .filter(|(_, m)| m.body.pos.distance(at) < 80.0)
                .map(|(id, m)| MobState { id: *id, kind: m.kind, pos: m.body.pos.to_array(), yaw: m.yaw, hurt: m.hurt > 0.0 })
                .collect();
            p.send(&ToClient::Mobs(list));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use runecraft_common::net::write_frame;

    #[test]
    fn a_client_joins_gets_chunks_and_sees_its_edit() {
        let dir = std::env::temp_dir().join(format!("runecraft-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let cfg = Config { bind: "127.0.0.1:0".into(), world_dir: dir.clone(), mobs: false, ..Default::default() };
        let mut server = start(cfg).unwrap();
        let mut s = TcpStream::connect(server.addr).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
        write_frame(&mut s, &ToServer::Hello { name: "tester".into(), version: PROTOCOL_VERSION, view_distance: 2, skin: 0 }).unwrap();
        let mut r = io::BufReader::new(s.try_clone().unwrap());
        let mut got_welcome = false;
        let mut columns = 0;
        while columns < 5 {
            match read_frame::<ToClient, _>(&mut r).unwrap() {
                ToClient::Welcome { .. } => got_welcome = true,
                ToClient::Column { data, .. } => {
                    assert!(Column::decode(&data).is_some());
                    columns += 1;
                }
                _ => {}
            }
        }
        assert!(got_welcome);
        write_frame(&mut s, &ToServer::SetBlock { pos: [0, 120, 0], block: block::GLASS }).unwrap();
        loop {
            if let ToClient::BlockChanged { pos, block: b } = read_frame::<ToClient, _>(&mut r).unwrap() {
                assert_eq!((pos, b), ([0, 120, 0], block::GLASS));
                break;
            }
        }
        drop(s);
        server.stop();
        assert!(dir.join("world.dat").exists(), "the edit was saved");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
