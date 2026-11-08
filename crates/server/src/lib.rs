//! The RuneCraft server: owns the world, streams chunks, relays players, simulates creatures,
//! items, arrows, fluids, crops, furnaces and weather, and saves everything to disk. Runs as its
//! own binary or inside the client for single player and LAN games.

mod containers;
mod entities;
mod fluids;

use containers::Container;
use entities::Entity;
use runecraft_common::block::{self, BlockId, Stack};
use runecraft_common::chunk::{self, CHUNK, Column, HEIGHT};
use runecraft_common::city::CITIES;
use runecraft_common::net::{encode_frame, read_frame};
use runecraft_common::noise::{hash, unit};
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

const SAVE_VERSION: u32 = 2;

#[derive(Serialize, Deserialize, Default)]
struct Save {
    version: u32,
    seed: u64,
    time: f32,
    edits: Vec<((i32, i32), Vec<(u32, u8)>)>,
    players: Vec<(String, [f32; 3])>,
    saves: Vec<(String, PlayerSave)>,
    spawns: Vec<(String, [f32; 3])>,
    containers: Vec<([i32; 3], Container)>,
}

pub(crate) struct Player {
    name: String,
    skin: u8,
    pose: Pose,
    out: Sender<Arc<Vec<u8>>>,
    view: u8,
    sent: HashSet<(i32, i32)>,
    hurt_cooldown: f32,
    held: u8,
    sleeping: bool,
    open: Option<[i32; 3]>,
    armor: f32,
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

pub(crate) struct World {
    cfg: Config,
    seed: u64,
    time: f32,
    columns: HashMap<(i32, i32), Loaded>,
    edits: HashMap<(i32, i32), HashMap<u32, u8>>,
    saved_pos: HashMap<String, [f32; 3]>,
    saves: HashMap<String, PlayerSave>,
    spawns: HashMap<String, [f32; 3]>,
    pending: HashSet<(i32, i32)>,
    jobs: Sender<(i32, i32)>,
    done: Receiver<(i32, i32, Column)>,
    players: HashMap<u32, Player>,
    entities: HashMap<u32, Entity>,
    next_entity: u32,
    containers: HashMap<[i32; 3], Container>,
    fluid_active: HashSet<[i32; 3]>,
    fluid_clock: (f32, f32),
    growing: HashSet<[i32; 3]>,
    fuses: Vec<(f32, [i32; 3])>,
    rain: f32,
    thunder: bool,
    weather_timer: f32,
    lightning_timer: f32,
    dirty: bool,
    rng: u64,
    spawn: [f32; 3],
}

impl World {
    fn open(cfg: Config) -> World {
        let path = cfg.world_dir.join("world.dat");
        let save: Save = match std::fs::read(&path) {
            Ok(bytes) => match bincode::deserialize::<Save>(&bytes) {
                Ok(s) if s.version == SAVE_VERSION => s,
                _ => {
                    // An older save format: keep it aside and start this world fresh.
                    let _ = std::fs::rename(&path, cfg.world_dir.join("world.dat.old"));
                    Save { version: SAVE_VERSION, seed: cfg.seed, time: 0.3, ..Default::default() }
                }
            },
            Err(_) => Save { version: SAVE_VERSION, seed: cfg.seed, time: 0.3, ..Default::default() },
        };
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
        World {
            seed,
            time: save.time,
            columns: HashMap::new(),
            edits,
            saved_pos: save.players.into_iter().collect(),
            saves: save.saves.into_iter().collect(),
            spawns: save.spawns.into_iter().collect(),
            pending: HashSet::new(),
            jobs,
            done,
            players: HashMap::new(),
            entities: HashMap::new(),
            next_entity: 1,
            containers: save.containers.into_iter().collect(),
            fluid_active: HashSet::new(),
            fluid_clock: (0.0, 0.0),
            growing: HashSet::new(),
            fuses: Vec::new(),
            rain: 0.0,
            thunder: false,
            weather_timer: 300.0,
            lightning_timer: 10.0,
            dirty: false,
            rng: hash(seed ^ 0xA11CE),
            spawn: worldgen::find_spawn(seed),
            cfg,
        }
    }

    pub(crate) fn rand(&mut self) -> f32 {
        self.rng = hash(self.rng.wrapping_add(0x9E3779B97F4A7C15));
        unit(self.rng)
    }

    fn save(&mut self) {
        let mut save = Save { version: SAVE_VERSION, seed: self.seed, time: self.time, ..Default::default() };
        save.edits = self.edits.iter().map(|(k, v)| (*k, v.iter().map(|(a, b)| (*a, *b)).collect())).collect();
        for p in self.players.values() {
            self.saved_pos.insert(p.name.clone(), p.pose.pos);
        }
        save.players = self.saved_pos.iter().map(|(k, v)| (k.clone(), *v)).collect();
        save.saves = self.saves.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        save.spawns = self.spawns.iter().map(|(k, v)| (k.clone(), *v)).collect();
        save.containers = self.containers.iter().map(|(k, v)| (*k, v.clone())).collect();
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
        let mut since_spawn = 0.0f32;
        let mut since_gc = 0.0f32;
        let mut since_grow = 0.0f32;
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
                            if matches!(b, block::WHEAT_0..=block::WHEAT_2 | block::SAPLING) {
                                let (lx, y, lz) = (i as i32 % CHUNK, i as i32 / (CHUNK * CHUNK), (i as i32 / CHUNK) % CHUNK);
                                self.growing.insert([cx * CHUNK + lx, y, cz * CHUNK + lz]);
                            }
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
            self.tick_fluids(dt);
            self.tick_containers(dt);
            self.tick_weather(dt);
            self.tick_entities(dt);
            since_grow += dt;
            if since_grow > 1.0 {
                since_grow = 0.0;
                self.tick_growth();
            }
            since_spawn += dt;
            if since_spawn > 1.0 {
                since_spawn = 0.0;
                self.spawn_creatures();
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

    pub(crate) fn broadcast(&self, msg: &ToClient) {
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

    fn tell(&self, conn: u32, text: impl Into<String>) {
        if let Some(p) = self.players.get(&conn) {
            p.send(&ToClient::Chat { from: None, text: text.into() });
        }
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
                let p = Player {
                    name: name.clone(),
                    skin,
                    pose,
                    out,
                    view: view.clamp(2, self.cfg.max_view),
                    sent: HashSet::new(),
                    hurt_cooldown: 0.0,
                    held: 0,
                    sleeping: false,
                    open: None,
                    armor: 0.0,
                };
                p.send(&ToClient::Welcome {
                    id: conn,
                    seed: self.seed,
                    spawn: pos,
                    time: self.time,
                    world: self.cfg.world_dir.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
                    motd: self.cfg.motd.clone(),
                    save: self.saves.get(&name).cloned(),
                });
                p.send(&ToClient::SpawnPoint(self.spawns.get(&name).copied().unwrap_or(self.spawn)));
                p.send(&ToClient::Weather { rain: self.rain, thunder: self.thunder });
                for (id, other) in &self.players {
                    p.send(&ToClient::PlayerJoined { id: *id, name: other.name.clone(), skin: other.skin, pose: other.pose });
                    p.send(&ToClient::PlayerHeld { id: *id, item: other.held });
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
                    if matches!(block, block::WHEAT_0..=block::WHEAT_2 | block::SAPLING) {
                        self.growing.insert(pos);
                    }
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
            ToServer::Attack { entity, damage } => self.attack(conn, entity, damage),
            ToServer::Interact { entity } => self.interact(conn, entity),
            ToServer::ViewDistance(v) => {
                let max = self.cfg.max_view;
                if let Some(p) = self.players.get_mut(&conn) {
                    p.view = v.clamp(2, max);
                }
            }
            ToServer::Travel { pos } => self.teleport(conn, pos),
            ToServer::Drop { stack, pos, vel } => {
                if stack.n > 0 {
                    // Thrown items can't be caught again straight away; block drops can.
                    let delay = if Vec3::from(vel).length() > 2.5 { 1.5 } else { 0.3 };
                    self.spawn_item(stack, Vec3::from(pos), Vec3::from(vel), delay);
                }
            }
            ToServer::Shoot { pos, vel } => self.spawn_arrow(Vec3::from(pos), Vec3::from(vel), Some(conn), true),
            ToServer::OpenContainer { pos } => self.open_container(conn, pos),
            ToServer::ContainerSet { pos, slot, stack } => self.set_container_slot(pos, slot, stack),
            ToServer::CloseContainer => {
                if let Some(p) = self.players.get_mut(&conn) {
                    p.open = None;
                }
            }
            ToServer::Sleep(bed) => self.sleep(conn, bed),
            ToServer::BoneMeal { pos } => self.grow(pos, true),
            ToServer::Held(item) => {
                if let Some(p) = self.players.get_mut(&conn) {
                    p.held = item;
                }
                self.broadcast_except(conn, &ToClient::PlayerHeld { id: conn, item });
            }
            ToServer::Save(save) => {
                if let Some(p) = self.players.get_mut(&conn) {
                    p.armor = save.armor.iter().flatten().filter_map(|s| block::armor(s.id)).map(|a| a.1 as f32).sum();
                    let name = p.name.clone();
                    self.saves.insert(name, save);
                    self.dirty = true;
                }
            }
        }
    }

    fn command(&mut self, conn: u32, text: &str) {
        let parts: Vec<&str> = text[1..].split_whitespace().collect();
        match parts.as_slice() {
            ["help"] => self.tell(conn, "Commands: /tp <city|x y z|player>, /spawn, /time <day|night|noon|midnight>, /weather <clear|rain|thunder>, /give <item> [count], /list, /seed, /cities"),
            ["list"] => {
                let names: Vec<_> = self.players.values().map(|p| p.name.clone()).collect();
                self.tell(conn, format!("{} online: {}", names.len(), names.join(", ")));
            }
            ["cities"] => {
                let names: Vec<_> = CITIES.iter().map(|c| c.name).collect();
                self.tell(conn, format!("Cities: {}. Travel with /tp <city>.", names.join(", ")));
            }
            ["seed"] => self.tell(conn, format!("Seed: {}", self.seed)),
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
            ["weather", w] => {
                let (rain, thunder) = match *w {
                    "rain" => (1.0, false),
                    "thunder" | "storm" => (1.0, true),
                    _ => (0.0, false),
                };
                self.set_weather(rain, thunder, 600.0);
                self.system(format!("Weather: {w}"));
            }
            ["give", rest @ ..] if !rest.is_empty() => {
                let (n, name_parts) = match rest.last().and_then(|n| n.parse::<u8>().ok()) {
                    Some(n) if rest.len() > 1 => (n, &rest[..rest.len() - 1]),
                    _ => (1, rest),
                };
                let want = name_parts.join("").to_lowercase().replace('_', "");
                let found = (1..block::ITEM_END).find(|&id| block::item_name(id).to_lowercase().replace(' ', "") == want);
                match found {
                    Some(id) => {
                        if let Some(p) = self.players.get(&conn) {
                            p.send(&ToClient::Pickup(Stack::new(id, n.clamp(1, block::max_stack(id)))));
                        }
                    }
                    None => self.tell(conn, format!("No item called {}", name_parts.join(" "))),
                }
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
                    self.tell(conn, format!("Welcome to {}!", c.name));
                } else if let Some(p) = self.players.values().find(|p| p.name.to_lowercase() == want) {
                    let pos = p.pose.pos;
                    self.teleport(conn, pos);
                } else {
                    self.tell(conn, format!("No city or player called {want}"));
                }
            }
            _ => self.tell(conn, format!("Unknown command: {text}. Try /help")),
        }
    }

    fn teleport(&mut self, conn: u32, pos: [f32; 3]) {
        if let Some(p) = self.players.get_mut(&conn) {
            p.pose.pos = pos;
            p.sleeping = false;
            p.send(&ToClient::Teleport { pos });
        }
    }

    pub(crate) fn get(&self, x: i32, y: i32, z: i32) -> BlockId {
        if !(0..HEIGHT).contains(&y) {
            return block::AIR;
        }
        let ((cx, lx), (cz, lz)) = (chunk::split(x), chunk::split(z));
        self.columns.get(&(cx, cz)).map_or(block::BEDROCK, |l| l.col.get(lx, y, lz))
    }

    pub(crate) fn loaded(&self, x: i32, z: i32) -> bool {
        self.columns.contains_key(&(x.div_euclid(CHUNK), z.div_euclid(CHUNK)))
    }

    pub(crate) fn set_block(&mut self, pos: [i32; 3], b: BlockId) {
        let [x, y, z] = pos;
        let ((cx, lx), (cz, lz)) = (chunk::split(x), chunk::split(z));
        let Some(l) = self.columns.get_mut(&(cx, cz)) else { return };
        let old = l.col.get(lx, y, lz);
        if old == b {
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
        // A container broken: its contents spill out.
        let was_container = matches!(old, block::CHEST | block::FURNACE | block::FURNACE_LIT);
        let still = matches!(b, block::CHEST | block::FURNACE | block::FURNACE_LIT);
        if was_container && !still {
            self.spill_container(pos);
        }
        if !matches!(b, block::WHEAT_0..=block::WHEAT_2 | block::SAPLING) {
            self.growing.remove(&pos);
        }
        // Doors come in pairs: keep both halves the same.
        if let Some((upper, open, axis)) = block::door_parts(b) {
            let other = [x, if upper { y - 1 } else { y + 1 }, z];
            if let Some((o_upper, o_open, o_axis)) = block::door_parts(self.get(other[0], other[1], other[2]))
                && o_upper != upper
                && (o_open != open || o_axis != axis)
            {
                self.set_block(other, block::door(!upper, open, axis));
            }
        } else if let Some((upper, _, _)) = block::door_parts(old) {
            let other = [x, if upper { y - 1 } else { y + 1 }, z];
            if block::door_parts(self.get(other[0], other[1], other[2])).is_some() {
                self.set_block(other, block::AIR);
            }
        }
        // Things that need support fall with it.
        let above = [x, y + 1, z];
        let up = self.get(above[0], above[1], above[2]);
        if !block::is_solid(b) && (matches!(block::def(up).kind, block::Kind::Plant) || matches!(up, block::TORCH | block::RAIL)) && block::fluid(b).is_none() {
            self.set_block(above, block::AIR);
        }
        self.wake_fluids(pos);
    }

    fn stream_chunks(&mut self) {
        let mut wanted: Vec<(i32, i32)> = Vec::new();
        let ids: Vec<u32> = self.players.keys().copied().collect();
        for id in ids {
            let (center, view) = {
                let p = &self.players[&id];
                (p.chunk(), p.view as i32)
            };
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
        let loaded: HashSet<(i32, i32)> = self.columns.keys().copied().collect();
        self.fluid_active.retain(|p| loaded.contains(&(p[0].div_euclid(CHUNK), p[2].div_euclid(CHUNK))));
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
                    if block::fluid(b).is_some() || matches!(b, block::AIR | block::BEDROCK | block::OBSIDIAN) {
                        continue;
                    }
                    if b == block::TNT {
                        self.set_block(p, block::AIR);
                        let fuse = 0.4 + self.rand() * 0.6;
                        self.fuses.push((fuse, p));
                        continue;
                    }
                    self.set_block(p, block::AIR);
                    // Some of what blew up lands as items.
                    let drop = block::def(b).drop;
                    if drop != block::AIR && self.rand() < 0.25 {
                        let at = Vec3::new(p[0] as f32 + 0.5, p[1] as f32 + 0.5, p[2] as f32 + 0.5);
                        let vel = (at - centre).normalize_or_zero() * 4.0 + Vec3::Y * 3.0;
                        self.spawn_item(Stack::new(drop, 1), at, vel, 0.5);
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
        let hits: Vec<u32> = self.entities.iter().filter(|(_, e)| e.body.pos.distance(centre) < 5.0 && e.kind.living()).map(|(id, _)| *id).collect();
        for id in hits {
            self.hurt_entity(id, 12.0, centre);
        }
    }

    pub(crate) fn is_night(&self) -> bool {
        !(0.23..0.77).contains(&self.time)
    }

    fn tick_weather(&mut self, dt: f32) {
        self.weather_timer -= dt;
        if self.weather_timer <= 0.0 {
            let r = self.rand();
            let (rain, thunder) = if self.rain > 0.0 { (0.0, false) } else if r < 0.25 { (1.0, true) } else { (1.0, false) };
            let next = 240.0 + self.rand() * 480.0;
            self.set_weather(rain, thunder, next);
        }
        if self.thunder {
            self.lightning_timer -= dt;
            if self.lightning_timer <= 0.0 {
                self.lightning_timer = 6.0 + self.rand() * 14.0;
                let ps: Vec<Vec3> = self.players.values().map(|p| p.pos()).collect();
                if ps.is_empty() {
                    return;
                }
                let at = ps[(self.rand() * ps.len() as f32) as usize % ps.len()];
                let a = self.rand() * std::f32::consts::TAU;
                let d = 8.0 + self.rand() * 60.0;
                let (x, z) = ((at.x + a.cos() * d).floor() as i32, (at.z + a.sin() * d).floor() as i32);
                if !self.loaded(x, z) {
                    return;
                }
                let ((cx, lx), (cz, lz)) = (chunk::split(x), chunk::split(z));
                let y = self.columns[&(cx, cz)].col.height(lx, lz) + 1;
                let strike = Vec3::new(x as f32 + 0.5, y as f32, z as f32 + 0.5);
                self.broadcast(&ToClient::Lightning { pos: strike.to_array() });
                for p in self.players.values() {
                    if p.pos().distance(strike) < 3.5 {
                        p.send(&ToClient::Damage { amount: 5.0, from: strike.to_array() });
                    }
                }
            }
        }
    }

    fn set_weather(&mut self, rain: f32, thunder: bool, duration: f32) {
        self.rain = rain;
        self.thunder = thunder;
        self.weather_timer = duration;
        self.broadcast(&ToClient::Weather { rain, thunder });
    }

    fn tick_growth(&mut self) {
        let list: Vec<[i32; 3]> = self.growing.iter().copied().collect();
        for pos in list {
            if !self.loaded(pos[0], pos[2]) {
                continue;
            }
            let chance = if self.get(pos[0], pos[1], pos[2]) == block::SAPLING { 1.0 / 90.0 } else { 1.0 / 25.0 };
            if self.rand() < chance {
                self.grow(pos, false);
            }
        }
    }

    /// Advances a crop a stage or turns a sapling into a tree.
    fn grow(&mut self, pos: [i32; 3], bone_meal: bool) {
        let b = self.get(pos[0], pos[1], pos[2]);
        match b {
            block::WHEAT_0..=block::WHEAT_2 => {
                let step = if bone_meal { 2 } else { 1 };
                let next = (b + step).min(block::WHEAT_3);
                self.set_block(pos, next);
                if next != block::WHEAT_3 {
                    self.growing.insert(pos);
                }
            }
            block::SAPLING => {
                self.growing.remove(&pos);
                let kind = if self.rand() < 0.3 { worldgen::TreeKind::Birch } else { worldgen::TreeKind::Oak };
                let tree = worldgen::Tree { x: pos[0], y: pos[1] - 1, z: pos[2], kind, height: 4 + (self.rand() * 3.0) as i32 };
                let mut puts = Vec::new();
                worldgen::tree_blocks(self.seed, &tree, &mut |x, y, z, b, force| puts.push(([x, y, z], b, force)));
                for (p, b, force) in puts {
                    if p == [pos[0], pos[1] - 1, pos[2]] {
                        continue;
                    }
                    let cur = self.get(p[0], p[1], p[2]);
                    if cur == block::AIR || cur == block::SAPLING || cur == block::TALL_GRASS || (force && block::def(cur).kind == block::Kind::Cutout) {
                        self.set_block(p, b);
                    }
                }
            }
            _ => {
                self.growing.remove(&pos);
            }
        }
    }

    fn sleep(&mut self, conn: u32, bed: Option<[i32; 3]>) {
        let Some(p) = self.players.get_mut(&conn) else { return };
        p.sleeping = bed.is_some();
        if let Some(b) = bed {
            let spawn = [b[0] as f32 + 0.5, b[1] as f32 + 1.0, b[2] as f32 + 0.5];
            let name = p.name.clone();
            p.send(&ToClient::SpawnPoint(spawn));
            self.spawns.insert(name, spawn);
            self.dirty = true;
        }
        let total = self.players.len();
        let asleep = self.players.values().filter(|p| p.sleeping).count();
        if bed.is_some() && total > 1 {
            self.system(format!("{asleep}/{total} players sleeping"));
        }
        if asleep == total && total > 0 && self.is_night() {
            self.time = 0.24;
            self.broadcast(&ToClient::Time(self.time));
            self.broadcast(&ToClient::WakeUp);
            if self.rain > 0.0 {
                self.set_weather(0.0, false, 600.0);
            }
            for p in self.players.values_mut() {
                p.sleeping = false;
            }
            self.system("Good morning!");
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
        // A dropped stack comes back when you stand on it.
        let spawn = worldgen::find_spawn(0x5EED);
        write_frame(&mut s, &ToServer::Move(Pose { pos: spawn, yaw: 0.0, pitch: 0.0 })).unwrap();
        write_frame(&mut s, &ToServer::Drop { stack: Stack::new(block::DIAMOND, 3), pos: spawn, vel: [0.0; 3] }).unwrap();
        loop {
            if let ToClient::Pickup(st) = read_frame::<ToClient, _>(&mut r).unwrap() {
                assert_eq!((st.id, st.n), (block::DIAMOND, 3));
                break;
            }
        }
        drop(s);
        server.stop();
        assert!(dir.join("world.dat").exists(), "the edit was saved");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn water_flows_down_and_out() {
        let dir = std::env::temp_dir().join(format!("runecraft-fluid-{}", std::process::id()));
        let cfg = Config { world_dir: dir, mobs: false, ..Default::default() };
        let mut w = World::open(cfg);
        let mut col = Column::default();
        for x in 0..16 {
            for z in 0..16 {
                col.set(x, 10, z, block::STONE);
            }
        }
        for cz in -1..=1 {
            for cx in -1..=1 {
                w.columns.insert((cx, cz), Loaded { col: col.clone(), encoded: None });
            }
        }
        w.set_block([8, 13, 8], block::WATER);
        for _ in 0..200 {
            w.tick_fluids(0.05);
        }
        assert_eq!(block::fluid(w.get(8, 11, 8)), Some((0, 7)), "a falling column reaches the floor");
        assert!(block::fluid(w.get(11, 11, 8)).is_some_and(|(_, l)| l < 7), "and spreads, thinning out");
        assert_eq!(w.get(8, 11, 20), block::AIR, "but only so far");
        w.set_block([8, 13, 8], block::AIR);
        for _ in 0..400 {
            w.tick_fluids(0.05);
        }
        assert_eq!(w.get(10, 11, 8), block::AIR, "without its source the water drains away");
    }
}
