//! The RuneCraft server: owns the world, streams chunks, relays players, simulates dropped
//! items, fluids, crops, furnaces and weather, and saves everything to disk. Runs as its own
//! binary or inside the client for single player and LAN games.

mod containers;
mod fluids;
mod items;
mod places;
mod portals;
mod redstone;
mod vehicles;

use containers::Container;
use items::Item;
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
}

impl Default for Config {
    fn default() -> Self {
        Config {
            bind: format!("0.0.0.0:{}", runecraft_common::DEFAULT_PORT),
            world_dir: PathBuf::from("worlds/world"),
            seed: 0x5EED,
            motd: "Welcome to RuneCraft!".into(),
            max_view: 12,
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

const SAVE_VERSION: u32 = 4;

/// The overworld, the Ember realm and the Sky realm.
pub const DIMENSIONS: usize = 3;

#[derive(Serialize, Deserialize, Default)]
struct LevelSave {
    edits: Vec<((i32, i32), Vec<(u32, u8)>)>,
    containers: Vec<([i32; 3], Container)>,
    vehicles: Vec<vehicles::Vehicle>,
}

#[derive(Serialize, Deserialize, Default)]
struct Save {
    version: u32,
    seed: u64,
    time: f32,
    levels: Vec<LevelSave>,
    players: Vec<(String, [f32; 3], u8)>,
    saves: Vec<(String, PlayerSave)>,
    spawns: Vec<(String, [f32; 3])>,
    places: Vec<places::PlaceInfo>,
}

pub(crate) struct Player {
    name: String,
    skin: u8,
    pose: Pose,
    out: Sender<Arc<Vec<u8>>>,
    view: u8,
    sent: HashSet<(i32, i32)>,
    held: u8,
    sleeping: bool,
    open: Option<[i32; 3]>,
    armor: f32,
    dim: u8,
    /// Seconds spent standing in a portal, and whether you must step out before it works again.
    portal: (f32, bool),
    riding: Option<(u32, u8)>,
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

/// Everything in one dimension: its blocks and what is happening in them.
#[derive(Default)]
pub(crate) struct Level {
    columns: HashMap<(i32, i32), Loaded>,
    edits: HashMap<(i32, i32), HashMap<u32, u8>>,
    pending: HashSet<(i32, i32)>,
    items: HashMap<u32, Item>,
    containers: HashMap<[i32; 3], Container>,
    fluid_active: HashSet<[i32; 3]>,
    fluid_clock: (f32, f32),
    growing: HashSet<[i32; 3]>,
    fuses: Vec<(f32, [i32; 3])>,
    pub(crate) rs: redstone::Redstone,
    pub(crate) vehicles: HashMap<u32, vehicles::Vehicle>,
    pub(crate) vehicles_dirty: bool,
    pub(crate) vehicle_clock: f32,
}

pub(crate) struct World {
    cfg: Config,
    seed: u64,
    time: f32,
    /// The dimension being worked on; its level is `lv`, the others wait in `parked`.
    dim: u8,
    lv: Level,
    parked: Vec<Level>,
    saved_pos: HashMap<String, ([f32; 3], u8)>,
    saves: HashMap<String, PlayerSave>,
    spawns: HashMap<String, [f32; 3]>,
    jobs: Sender<(u8, i32, i32)>,
    done: Receiver<(u8, i32, i32, Column)>,
    players: HashMap<u32, Player>,
    next_item: u32,
    merge_clock: f32,
    places: Arc<std::sync::RwLock<Vec<Arc<runecraft_common::osm::Place>>>>,
    place_info: Vec<places::PlaceInfo>,
    place_results: (Sender<places::Loaded>, Receiver<places::Loaded>),
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
        let mut levels: Vec<Level> = (0..DIMENSIONS).map(|_| Level::default()).collect();
        for (lv, ls) in levels.iter_mut().zip(save.levels) {
            lv.edits = ls.edits.into_iter().map(|(k, v)| (k, v.into_iter().collect())).collect();
            lv.containers = ls.containers.into_iter().collect();
            lv.vehicles = ls.vehicles.into_iter().map(|v| (v.id, v)).collect();
        }
        let next_item = levels.iter().flat_map(|l| l.vehicles.keys()).max().map_or(1, |m| m + 1);
        let place_list: Vec<Arc<runecraft_common::osm::Place>> = save.places.iter().filter_map(|p| places::load(&cfg.world_dir, p)).map(Arc::new).collect();
        let places = Arc::new(std::sync::RwLock::new(place_list));
        let (jobs, job_rx) = mpsc::channel::<(u8, i32, i32)>();
        let (done_tx, done) = mpsc::channel();
        let job_rx = Arc::new(Mutex::new(job_rx));
        let workers = thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(2, 8) - 1;
        for i in 0..workers.max(1) {
            let job_rx = job_rx.clone();
            let done_tx = done_tx.clone();
            let places = places.clone();
            let _ = thread::Builder::new().name(format!("gen-{i}")).spawn(move || {
                loop {
                    let job = job_rx.lock().unwrap().recv();
                    let Ok((dim, cx, cz)) = job else { return };
                    let list = places.read().map(|l| l.clone()).unwrap_or_default();
                    let col = worldgen::generate_in(seed, dim, cx, cz, &list);
                    if done_tx.send((dim, cx, cz, col)).is_err() {
                        return;
                    }
                }
            });
        }
        let lv = std::mem::take(&mut levels[0]);
        World {
            seed,
            time: save.time,
            dim: 0,
            lv,
            parked: levels,
            saved_pos: save.players.into_iter().map(|(n, p, d)| (n, (p, d))).collect(),
            saves: save.saves.into_iter().collect(),
            spawns: save.spawns.into_iter().collect(),
            jobs,
            done,
            players: HashMap::new(),
            next_item,
            merge_clock: 0.0,
            places,
            place_info: save.places,
            place_results: mpsc::channel(),
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
        for d in 0..DIMENSIONS as u8 {
            let lv = self.level(d);
            save.levels.push(LevelSave {
                edits: lv.edits.iter().map(|(k, v)| (*k, v.iter().map(|(a, b)| (*a, *b)).collect())).collect(),
                containers: lv.containers.iter().map(|(k, v)| (*k, v.clone())).collect(),
                vehicles: lv.vehicles.values().cloned().collect(),
            });
        }
        for p in self.players.values() {
            self.saved_pos.insert(p.name.clone(), (p.pose.pos, p.dim));
        }
        save.players = self.saved_pos.iter().map(|(k, v)| (k.clone(), v.0, v.1)).collect();
        save.saves = self.saves.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        save.spawns = self.spawns.iter().map(|(k, v)| (k.clone(), *v)).collect();
        save.places = self.place_info.clone();
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
            while let Ok((dim, cx, cz, col)) = self.done.try_recv() {
                self.enter(dim);
                self.lv.pending.remove(&(cx, cz));
                self.insert_column(cx, cz, col);
            }
            self.poll_places();
            self.time = (self.time + dt / DAY_LENGTH).fract();
            since_time += dt;
            if since_time > 5.0 {
                since_time = 0.0;
                self.broadcast(&ToClient::Time(self.time));
            }
            since_grow += dt;
            since_gc += dt;
            for d in 0..DIMENSIONS as u8 {
                self.enter(d);
                let here = self.players.values().any(|p| p.dim == d);
                if !here && self.lv.columns.is_empty() {
                    continue;
                }
                self.stream_chunks();
                self.tick_fuses(dt);
                self.tick_fluids(dt);
                self.tick_containers(dt);
                self.tick_items(dt);
                self.tick_redstone(dt);
                self.tick_vehicles(dt);
                self.tick_portals(dt);
                if since_grow > 1.0 {
                    self.tick_growth();
                }
                if since_gc > 10.0 {
                    self.unload_far();
                }
            }
            self.enter(0);
            self.tick_weather(dt);
            if since_grow > 1.0 {
                since_grow = 0.0;
            }
            if since_gc > 10.0 {
                since_gc = 0.0;
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

    /// Switches the world to dimension `d`: its level becomes `self.lv`.
    pub(crate) fn enter(&mut self, d: u8) {
        if d == self.dim || d as usize >= DIMENSIONS {
            return;
        }
        let cur = self.dim as usize;
        std::mem::swap(&mut self.lv, &mut self.parked[cur]);
        std::mem::swap(&mut self.lv, &mut self.parked[d as usize]);
        self.dim = d;
    }

    fn level(&self, d: u8) -> &Level {
        if d == self.dim { &self.lv } else { &self.parked[d as usize] }
    }

    /// A freshly generated column joins the current level, with the player edits made to it.
    fn insert_column(&mut self, cx: i32, cz: i32, mut col: Column) {
        if let Some(e) = self.lv.edits.get(&(cx, cz)) {
            for (&i, &b) in e {
                if (i as usize) < col.blocks.len() {
                    col.blocks[i as usize] = b;
                }
            }
            col.recompute_heights();
        }
        // Growing crops and redstone in it come back to life.
        let edited: Vec<(u32, u8)> = self.lv.edits.get(&(cx, cz)).map(|e| e.iter().map(|(a, b)| (*a, *b)).collect()).unwrap_or_default();
        for (i, b) in edited {
            let (lx, y, lz) = (i as i32 % CHUNK, i as i32 / (CHUNK * CHUNK), (i as i32 / CHUNK) % CHUNK);
            let pos = [cx * CHUNK + lx, y, cz * CHUNK + lz];
            if matches!(b, block::WHEAT_0..=block::WHEAT_2 | block::SAPLING) {
                self.lv.growing.insert(pos);
            }
            if block::is_redstone(b) {
                self.lv.rs.track(pos);
            }
        }
        self.lv.columns.insert((cx, cz), Loaded { col, encoded: None });
    }

    /// Generates a column now, on this thread, if it isn't loaded (for portals and travel).
    pub(crate) fn ensure_column(&mut self, cx: i32, cz: i32) {
        if self.lv.columns.contains_key(&(cx, cz)) {
            return;
        }
        let list = self.places.read().map(|l| l.clone()).unwrap_or_default();
        let col = worldgen::generate_in(self.seed, self.dim, cx, cz, &list);
        self.lv.pending.remove(&(cx, cz));
        self.insert_column(cx, cz, col);
    }

    /// To every player in the current dimension.
    pub(crate) fn broadcast_dim(&self, msg: &ToClient) {
        let frame = Arc::new(encode_frame(msg));
        for p in self.players.values().filter(|p| p.dim == self.dim) {
            let _ = p.out.send(frame.clone());
        }
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
            if *id != except && p.dim == self.dim {
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
                let (pos, dim) = self.saved_pos.get(&name).copied().unwrap_or((self.spawn, 0));
                let dim = dim.min(DIMENSIONS as u8 - 1);
                let pose = Pose { pos, yaw: 0.0, pitch: 0.0 };
                let p = Player {
                    name: name.clone(),
                    skin,
                    pose,
                    out,
                    view: view.clamp(2, self.cfg.max_view),
                    sent: HashSet::new(),
                    held: 0,
                    sleeping: false,
                    open: None,
                    armor: 0.0,
                    dim,
                    portal: (0.0, true),
                    riding: None,
                };
                p.send(&ToClient::Welcome {
                    id: conn,
                    seed: self.seed,
                    dim,
                    spawn: pos,
                    time: self.time,
                    world: self.cfg.world_dir.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
                    motd: self.cfg.motd.clone(),
                    save: self.saves.get(&name).cloned(),
                });
                p.send(&ToClient::SpawnPoint(self.spawns.get(&name).copied().unwrap_or(self.spawn)));
                p.send(&ToClient::Weather { rain: self.rain, thunder: self.thunder });
                p.send(&self.places_message());
                for (id, other) in self.players.iter().filter(|(_, o)| o.dim == dim) {
                    p.send(&ToClient::PlayerJoined { id: *id, name: other.name.clone(), skin: other.skin, pose: other.pose });
                    p.send(&ToClient::PlayerHeld { id: *id, item: other.held });
                }
                let joined = encode_frame(&ToClient::PlayerJoined { id: conn, name: name.clone(), skin, pose });
                for o in self.players.values().filter(|o| o.dim == dim) {
                    let _ = o.out.send(Arc::new(joined.clone()));
                }
                self.enter(dim);
                p.send(&self.vehicles_message());
                self.players.insert(conn, p);
                self.system(format!("{name} joined the game"));
            }
            Event::Leave(conn) => {
                if let Some(p) = self.players.remove(&conn) {
                    self.saved_pos.insert(p.name.clone(), (p.pose.pos, p.dim));
                    self.enter(p.dim);
                    self.dismount(conn, p.riding);
                    self.dirty = true;
                    self.broadcast(&ToClient::PlayerLeft { id: conn });
                    self.system(format!("{} left the game", p.name));
                }
            }
            Event::Msg(conn, msg) => self.message(conn, msg),
        }
    }

    fn message(&mut self, conn: u32, msg: ToServer) {
        let Some(dim) = self.players.get(&conn).map(|p| p.dim) else { return };
        self.enter(dim);
        match msg {
            ToServer::Hello { .. } => {}
            ToServer::Move(pose) => {
                if let Some(p) = self.players.get_mut(&conn) {
                    p.pose = pose;
                }
                self.broadcast_except(conn, &ToClient::PlayerMoved { id: conn, pose });
            }
            ToServer::Light { pos } => self.light(pos),
            ToServer::PlaceVehicle { kind, pos, yaw } => self.place_vehicle(kind, pos, yaw),
            ToServer::Mount { id, seat } => self.mount(conn, id, seat),
            ToServer::Dismount => {
                let r = self.players.get(&conn).and_then(|p| p.riding);
                self.dismount(conn, r);
            }
            ToServer::VehicleMove { id, pos, yaw, speed, cars } => self.vehicle_moved(conn, id, pos, yaw, speed, cars),
            ToServer::BreakVehicle { id } => self.break_vehicle(conn, id),
            ToServer::LoadPlace(query) => self.load_place(conn, query),
            ToServer::SetBlock { pos, block } => {
                if (0..HEIGHT).contains(&pos[1]) && (block as usize) < block::BLOCK_COUNT {
                    self.set_block(pos, block);
                    if matches!(block, block::WHEAT_0..=block::WHEAT_2 | block::SAPLING) {
                        self.lv.growing.insert(pos);
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
                    self.lv.fuses.push((3.0, pos));
                }
            }
            ToServer::ViewDistance(v) => {
                let max = self.cfg.max_view;
                if let Some(p) = self.players.get_mut(&conn) {
                    p.view = v.clamp(2, max);
                }
            }
            ToServer::Travel { pos } => self.change_dim(conn, 0, pos),
            ToServer::Drop { stack, pos, vel } => {
                if stack.n > 0 {
                    // Thrown items can't be caught again straight away; block drops can.
                    let delay = if Vec3::from(vel).length() > 2.5 { 1.5 } else { 0.3 };
                    self.spawn_item(stack, Vec3::from(pos), Vec3::from(vel), delay);
                }
            }
            ToServer::OpenContainer { pos } => self.open_container(conn, pos),
            ToServer::ContainerSet { pos, slot, stack } => self.set_container_slot(pos, slot, stack),
            ToServer::CloseContainer => {
                if let Some(p) = self.players.get_mut(&conn) {
                    p.open = None;
                }
            }
            ToServer::Sleep(bed) => self.sleep(conn, bed),
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
            ["help"] => self.tell(conn, "Commands: /tp <city|x y z|player|place n>, /spawn, /time <day|night|noon|midnight>, /weather <clear|rain|thunder>, /give <item> [count], /list, /seed, /cities, /place <real place>, /places, /realm <overworld|ember|sky>"),
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
                self.change_dim(conn, 0, pos);
            }
            ["place", rest @ ..] if !rest.is_empty() => self.load_place(conn, rest.join(" ")),
            ["places"] => {
                let names: Vec<String> = self.place_info.iter().enumerate().map(|(i, p)| format!("{} {}", i + 1, p.name)).collect();
                if names.is_empty() {
                    self.tell(conn, "No real places loaded yet. Try /place Times Square, New York");
                } else {
                    self.tell(conn, format!("Real places: {}. Travel with /tp place <number>.", names.join(", ")));
                }
            }
            ["tp", "place", n] => match n.parse::<usize>().ok().and_then(|n| self.place_info.get(n.wrapping_sub(1))) {
                Some(p) => {
                    let pos = p.arrival();
                    self.change_dim(conn, 0, pos);
                }
                None => self.tell(conn, "No such place. See /places"),
            },
            ["realm", r] => {
                let d = match *r {
                    "ember" => 1,
                    "sky" => 2,
                    _ => 0,
                };
                self.send_through_portal(conn, d);
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
                    self.change_dim(conn, 0, [x, y, z]);
                    self.tell(conn, format!("Welcome to {}!", c.name));
                } else if let Some(p) = self.players.values().find(|p| p.name.to_lowercase() == want) {
                    let (pos, d) = (p.pose.pos, p.dim);
                    self.change_dim(conn, d, pos);
                } else {
                    self.tell(conn, format!("No city or player called {want}"));
                }
            }
            _ => self.tell(conn, format!("Unknown command: {text}. Try /help")),
        }
    }

    fn teleport(&mut self, conn: u32, pos: [f32; 3]) {
        let r = self.players.get(&conn).and_then(|p| p.riding);
        self.dismount(conn, r);
        if let Some(p) = self.players.get_mut(&conn) {
            p.pose.pos = pos;
            p.sleeping = false;
            p.send(&ToClient::Teleport { pos });
        }
    }

    /// Moves a player to another dimension (through a portal, or back to the overworld).
    pub(crate) fn change_dim(&mut self, conn: u32, d: u8, pos: [f32; 3]) {
        let Some(old) = self.players.get(&conn).map(|p| p.dim) else { return };
        if old == d {
            self.teleport(conn, pos);
            return;
        }
        self.enter(old);
        let r = self.players.get(&conn).and_then(|p| p.riding);
        self.dismount(conn, r);
        // Players in the old dimension stop seeing each other; those in the new one meet.
        let left = Arc::new(encode_frame(&ToClient::PlayerLeft { id: conn }));
        let mut meet = Vec::new();
        for (id, o) in &self.players {
            if *id == conn {
                continue;
            }
            if o.dim == old {
                let _ = o.out.send(left.clone());
                if let Some(me) = self.players.get(&conn) {
                    me.send(&ToClient::PlayerLeft { id: *id });
                }
            } else if o.dim == d {
                meet.push(*id);
            }
        }
        let Some(me) = self.players.get_mut(&conn) else { return };
        me.dim = d;
        me.sent.clear();
        me.pose.pos = pos;
        me.sleeping = false;
        me.open = None;
        me.send(&ToClient::Dimension { dim: d, pos });
        let (name, skin, pose, held) = (me.name.clone(), me.skin, me.pose, me.held);
        for id in meet {
            let o = &self.players[&id];
            o.send(&ToClient::PlayerJoined { id: conn, name: name.clone(), skin, pose });
            o.send(&ToClient::PlayerHeld { id: conn, item: held });
            let me = &self.players[&conn];
            me.send(&ToClient::PlayerJoined { id, name: o.name.clone(), skin: o.skin, pose: o.pose });
            me.send(&ToClient::PlayerHeld { id, item: o.held });
        }
        self.enter(d);
        let msg = self.vehicles_message();
        self.players[&conn].send(&msg);
    }

    pub(crate) fn get(&self, x: i32, y: i32, z: i32) -> BlockId {
        if !(0..HEIGHT).contains(&y) {
            return block::AIR;
        }
        let ((cx, lx), (cz, lz)) = (chunk::split(x), chunk::split(z));
        self.lv.columns.get(&(cx, cz)).map_or(block::BEDROCK, |l| l.col.get(lx, y, lz))
    }

    pub(crate) fn loaded(&self, x: i32, z: i32) -> bool {
        self.lv.columns.contains_key(&(x.div_euclid(CHUNK), z.div_euclid(CHUNK)))
    }

    pub(crate) fn set_block(&mut self, pos: [i32; 3], b: BlockId) {
        let [x, y, z] = pos;
        let ((cx, lx), (cz, lz)) = (chunk::split(x), chunk::split(z));
        let Some(l) = self.lv.columns.get_mut(&(cx, cz)) else { return };
        let old = l.col.get(lx, y, lz);
        if old == b {
            return;
        }
        l.col.set(lx, y, lz, b);
        l.encoded = None;
        self.lv.edits.entry((cx, cz)).or_default().insert(chunk::index(lx, y, lz) as u32, b);
        self.dirty = true;
        let frame = Arc::new(encode_frame(&ToClient::BlockChanged { pos, block: b }));
        for p in self.players.values() {
            if p.dim == self.dim && p.sent.contains(&(cx, cz)) {
                let _ = p.out.send(frame.clone());
            }
        }
        if block::is_redstone(b) || block::is_redstone(old) {
            self.lv.rs.changed(pos, b);
        } else {
            self.lv.rs.neighbour_changed(pos);
        }
        if matches!(old, block::EMBER_PORTAL | block::SKY_PORTAL | block::OBSIDIAN | block::GLOWSTONE) && !matches!(b, block::EMBER_PORTAL | block::SKY_PORTAL) {
            self.break_portal(pos);
        }
        // A container broken: its contents spill out.
        let was_container = matches!(old, block::CHEST | block::FURNACE | block::FURNACE_LIT);
        let still = matches!(b, block::CHEST | block::FURNACE | block::FURNACE_LIT);
        if was_container && !still {
            self.spill_container(pos);
        }
        if !matches!(b, block::WHEAT_0..=block::WHEAT_2 | block::SAPLING) {
            self.lv.growing.remove(&pos);
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
        if !block::is_solid(b) && block::needs_support(up) && block::fluid(b).is_none() {
            self.set_block(above, block::AIR);
        }
        self.wake_fluids(pos);
    }

    fn stream_chunks(&mut self) {
        let mut wanted: Vec<(i32, i32)> = Vec::new();
        let ids: Vec<u32> = self.players.iter().filter(|(_, p)| p.dim == self.dim).map(|(id, _)| *id).collect();
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
                match self.lv.columns.get_mut(&(cx, cz)) {
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
            if self.lv.pending.len() > 64 {
                break;
            }
            if self.lv.pending.insert(k) {
                let _ = self.jobs.send((self.dim, k.0, k.1));
            }
        }
    }

    fn unload_far(&mut self) {
        let centers: Vec<((i32, i32), i32)> = self.players.values().filter(|p| p.dim == self.dim).map(|p| (p.chunk(), p.view as i32 + 4)).collect();
        self.lv.columns.retain(|&(cx, cz), _| centers.iter().any(|&((x, z), r)| (cx - x).abs().max((cz - z).abs()) <= r));
        let loaded: HashSet<(i32, i32)> = self.lv.columns.keys().copied().collect();
        self.lv.fluid_active.retain(|p| loaded.contains(&(p[0].div_euclid(CHUNK), p[2].div_euclid(CHUNK))));
        self.lv.rs.retain_loaded(&loaded);
    }

    fn tick_fuses(&mut self, dt: f32) {
        for f in &mut self.lv.fuses {
            f.0 -= dt;
        }
        let due: Vec<[i32; 3]> = self.lv.fuses.iter().filter(|f| f.0 <= 0.0).map(|f| f.1).collect();
        self.lv.fuses.retain(|f| f.0 > 0.0);
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
                        self.lv.fuses.push((fuse, p));
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
        self.broadcast_dim(&ToClient::Explosion { pos: centre.to_array() });
        for p in self.players.values().filter(|p| p.dim == self.dim) {
            let d = p.pos().distance(centre);
            if d < 8.0 {
                p.send(&ToClient::Damage { amount: (8.0 - d) * 2.5, from: centre.to_array() });
            }
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
                let ps: Vec<Vec3> = self.players.values().filter(|p| p.dim == 0).map(|p| p.pos()).collect();
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
                let y = self.lv.columns[&(cx, cz)].col.height(lx, lz) + 1;
                let strike = Vec3::new(x as f32 + 0.5, y as f32, z as f32 + 0.5);
                self.broadcast_dim(&ToClient::Lightning { pos: strike.to_array() });
                for p in self.players.values().filter(|p| p.dim == 0) {
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
        let list: Vec<[i32; 3]> = self.lv.growing.iter().copied().collect();
        for pos in list {
            if !self.loaded(pos[0], pos[2]) {
                continue;
            }
            let chance = if self.get(pos[0], pos[1], pos[2]) == block::SAPLING { 1.0 / 90.0 } else { 1.0 / 25.0 };
            if self.rand() < chance {
                self.grow(pos);
            }
        }
    }

    /// Advances a crop a stage or turns a sapling into a tree.
    fn grow(&mut self, pos: [i32; 3]) {
        let b = self.get(pos[0], pos[1], pos[2]);
        match b {
            block::WHEAT_0..=block::WHEAT_2 => {
                let next = (b + 1).min(block::WHEAT_3);
                self.set_block(pos, next);
                if next != block::WHEAT_3 {
                    self.lv.growing.insert(pos);
                }
            }
            block::SAPLING => {
                self.lv.growing.remove(&pos);
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
                self.lv.growing.remove(&pos);
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
        let total = self.players.values().filter(|p| p.dim == 0).count();
        let asleep = self.players.values().filter(|p| p.sleeping && p.dim == 0).count();
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
        let cfg = Config { bind: "127.0.0.1:0".into(), world_dir: dir.clone(), ..Default::default() };
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
    fn a_player_rides_a_vehicle_and_crosses_to_the_sky_realm() {
        let dir = std::env::temp_dir().join(format!("runecraft-realm-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let cfg = Config { bind: "127.0.0.1:0".into(), world_dir: dir.clone(), ..Default::default() };
        let mut server = start(cfg).unwrap();
        let mut s = TcpStream::connect(server.addr).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(30))).unwrap();
        write_frame(&mut s, &ToServer::Hello { name: "rider".into(), version: PROTOCOL_VERSION, view_distance: 2, skin: 0 }).unwrap();
        let mut r = io::BufReader::new(s.try_clone().unwrap());
        let mut spawn = [0.0; 3];
        loop {
            if let ToClient::Welcome { spawn: sp, dim, .. } = read_frame::<ToClient, _>(&mut r).unwrap() {
                assert_eq!(dim, 0);
                spawn = sp;
                break;
            }
        }
        // A boat put down beside you: get in, and the server knows you're its driver.
        write_frame(&mut s, &ToServer::PlaceVehicle { kind: VehicleKind::Boat, pos: spawn, yaw: 0.0 }).unwrap();
        let id = loop {
            if let ToClient::Vehicles(v) = read_frame::<ToClient, _>(&mut r).unwrap()
                && let Some(b) = v.first()
            {
                break b.id;
            }
        };
        write_frame(&mut s, &ToServer::Mount { id, seat: 0 }).unwrap();
        write_frame(&mut s, &ToServer::VehicleMove { id, pos: [spawn[0] + 3.0, spawn[1], spawn[2]], yaw: 1.0, speed: 2.0, cars: vec![] }).unwrap();
        loop {
            if let ToClient::Vehicles(v) = read_frame::<ToClient, _>(&mut r).unwrap()
                && v.iter().any(|b| b.riders == vec![(0, 1)] && (b.pos[0] - spawn[0] - 3.0).abs() < 0.01)
            {
                break;
            }
        }
        // Off to the Sky realm: a portal is built there and its chunks arrive.
        write_frame(&mut s, &ToServer::Chat("/realm sky".into())).unwrap();
        let mut arrived = None;
        let mut columns = 0;
        while columns < 3 {
            match read_frame::<ToClient, _>(&mut r).unwrap() {
                ToClient::Dimension { dim, pos } => arrived = Some((dim, pos)),
                ToClient::Column { .. } if arrived.is_some() => columns += 1,
                _ => {}
            }
        }
        let (dim, pos) = arrived.unwrap();
        assert_eq!(dim, 2);
        assert!(pos[1] > 40.0, "you arrive up among the islands");
        drop(s);
        server.stop();
        let w = World::open(Config { world_dir: dir.clone(), ..Default::default() });
        assert_eq!(w.saved_pos.get("rider").map(|p| p.1), Some(2), "and the world remembers you're there");
        assert!(w.level(0).vehicles.values().any(|v| v.kind == VehicleKind::Boat), "the boat is saved");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn water_flows_down_and_out() {
        let dir = std::env::temp_dir().join(format!("runecraft-fluid-{}", std::process::id()));
        let cfg = Config { world_dir: dir, ..Default::default() };
        let mut w = World::open(cfg);
        let mut col = Column::default();
        for x in 0..16 {
            for z in 0..16 {
                col.set(x, 10, z, block::STONE);
            }
        }
        for cz in -1..=1 {
            for cx in -1..=1 {
                w.lv.columns.insert((cx, cz), Loaded { col: col.clone(), encoded: None });
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
