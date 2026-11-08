//! Game state and the per-frame update: networking, movement, mining, building, survival.

use crate::net::{self, Connection, NetEvent};
use crate::textures::{self, Atlas};
use crate::world::World;
use cubeterra_common::block::{self, BlockId};
use cubeterra_common::chunk::{CHUNK, Column, HEIGHT};
use cubeterra_common::city::CITIES;
use cubeterra_common::craft::{RECIPES, Recipe, Station};
use cubeterra_common::physics::{self, Body, Hit, raycast};
use cubeterra_common::protocol::*;
use glam::{IVec3, Mat4, Vec3};
use rune::{Pixels, TextEdit};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Screen {
    Title,
    Connecting,
    Playing,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Panel {
    None,
    Pause,
    Settings,
    Inventory,
    Travel,
    Chat,
    Dead,
    Help,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Survival,
    Creative,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stack {
    pub id: BlockId,
    pub n: u8,
}

pub struct Other {
    pub name: String,
    pub skin: u8,
    pub target: Pose,
    pub pos: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub walk: f32,
    pub speed: f32,
}

pub struct MobView {
    pub kind: MobKind,
    pub target: Vec3,
    pub pos: Vec3,
    pub yaw: f32,
    pub walk: f32,
    pub speed: f32,
    pub hurt: bool,
}

#[derive(Default)]
pub struct Look {
    pub down: bool,
    pub dragging: bool,
    pub last: (f32, f32),
}

pub struct ChatLine {
    pub at: f64,
    pub text: String,
    pub system: bool,
}

pub const INV: usize = 36;

pub struct State {
    pub screen: Screen,
    pub panel: Panel,
    pub name: TextEdit,
    pub addr: TextEdit,
    pub world_name: TextEdit,
    pub seed_text: TextEdit,
    pub chat_edit: TextEdit,
    pub status: String,
    pub server: Option<cubeterra_server::Server>,
    pub lan: bool,
    pub conn: Option<Connection>,
    pub waker: Option<rune::Waker>,

    pub world: World,
    pub atlas: Arc<Atlas>,
    pub icons: HashMap<BlockId, Pixels>,
    pub my_id: PlayerId,
    pub seed: u64,
    pub motd: String,
    pub world_label: String,
    pub spawn: Vec3,
    pub got_welcome: bool,

    pub body: Body,
    pub yaw: f32,
    pub pitch: f32,
    pub flying: bool,
    pub mode: Mode,
    pub sprint: bool,
    pub health: f32,
    pub hunger: f32,
    pub air: f32,
    pub hurt_flash: f32,
    pub shake: f32,
    pub regen: f32,

    pub inv: [Option<Stack>; INV],
    pub selected: usize,
    pub held: HashSet<String>,
    pub shift: bool,
    pub ctrl: bool,
    pub look: Look,
    pub mining: Option<(IVec3, f32)>,
    pub swing: f32,
    pub break_cooldown: f32,
    pub target: Option<Hit>,
    pub others: HashMap<PlayerId, Other>,
    pub mobs: HashMap<u32, MobView>,
    pub chat: Vec<ChatLine>,
    pub time_of_day: f32,
    pub t: f64,
    pub last_t: f64,
    pub frame_dt: f32,
    pub fps: f32,
    pub screen_px: (u32, u32),
    pub wheel_acc: f32,
    pub view_proj: Mat4,
    pub show_debug: bool,
    pub view_distance: u8,
    pub fov: f32,
    pub sensitivity: f32,
    pub last_move: (Pose, f64),
    pub space_tap: f64,
    pub explosions: Vec<(Vec3, f64)>,
    pub skin: u8,
    pub inv_page: usize,
    pub hint_until: f64,
}

fn data_dir() -> PathBuf {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"));
    match home {
        Some(h) => PathBuf::from(h).join(".cubeterra"),
        None => PathBuf::from("cubeterra-data"),
    }
}

fn starter_hotbar(mode: Mode) -> [Option<Stack>; INV] {
    let mut inv = [None; INV];
    if mode == Mode::Creative {
        let picks = [block::GRASS, block::STONE, block::PLANKS, block::BRICK, block::GLASS, block::OAK_LOG, block::LANTERN, block::WINDOW_BLUE, block::TNT];
        for (i, b) in picks.iter().enumerate() {
            inv[i] = Some(Stack { id: *b, n: 64 });
        }
    }
    inv
}

impl State {
    pub fn new() -> State {
        let atlas = Arc::new(textures::build_atlas());
        let mut icons = HashMap::new();
        for id in (1..block::BLOCK_COUNT as u8).chain(block::STICK..block::ITEM_END) {
            let p = Pixels::new();
            p.set(32, 32, atlas.icon(id));
            icons.insert(id, p);
        }
        let user = std::env::var("USER").or_else(|_| std::env::var("USERNAME")).unwrap_or_else(|_| "Steve".into());
        let name: String = user.chars().filter(|c| c.is_alphanumeric()).take(16).collect();
        State {
            screen: Screen::Title,
            panel: Panel::None,
            name: TextEdit::new(if name.is_empty() { "Player".to_string() } else { name }),
            addr: TextEdit::new("127.0.0.1"),
            world_name: TextEdit::new("My World"),
            seed_text: TextEdit::new(""),
            chat_edit: TextEdit::new(""),
            status: String::new(),
            server: None,
            lan: false,
            conn: None,
            waker: None,
            world: World::new(),
            atlas,
            icons,
            my_id: 0,
            seed: 0,
            motd: String::new(),
            world_label: String::new(),
            spawn: Vec3::ZERO,
            got_welcome: false,
            body: Body::new(Vec3::new(0.5, 80.0, 0.5)),
            yaw: 0.0,
            pitch: -0.2,
            flying: false,
            mode: Mode::Creative,
            sprint: false,
            health: 20.0,
            hunger: 20.0,
            air: 10.0,
            hurt_flash: 0.0,
            shake: 0.0,
            regen: 0.0,
            inv: starter_hotbar(Mode::Creative),
            selected: 0,
            held: HashSet::new(),
            shift: false,
            ctrl: false,
            look: Look::default(),
            mining: None,
            swing: 0.0,
            break_cooldown: 0.0,
            target: None,
            others: HashMap::new(),
            mobs: HashMap::new(),
            chat: Vec::new(),
            time_of_day: 0.3,
            t: 0.0,
            last_t: 0.0,
            frame_dt: 0.016,
            fps: 60.0,
            screen_px: (1280, 800),
            wheel_acc: 0.0,
            view_proj: Mat4::IDENTITY,
            show_debug: false,
            view_distance: 8,
            fov: 75.0,
            sensitivity: 1.0,
            last_move: (Pose { pos: [0.0; 3], yaw: 0.0, pitch: 0.0 }, 0.0),
            space_tap: -1.0,
            explosions: Vec::new(),
            skin: 0,
            inv_page: 0,
            hint_until: 0.0,
        }
    }

    // -----------------------------------------------------------------------------------------
    // Starting and leaving games

    fn connect_to(&mut self, addr: String) {
        self.world.clear();
        self.others.clear();
        self.mobs.clear();
        self.got_welcome = false;
        self.status = format!("Connecting to {addr}…");
        self.skin = (cubeterra_common::noise::hash(self.name.text.len() as u64 ^ self.name.text.bytes().map(|b| b as u64).sum::<u64>()) % 32) as u8;
        self.conn = Some(net::connect(addr, self.name.text.trim().to_string(), self.view_distance, self.skin, self.waker.clone()));
        self.screen = Screen::Connecting;
        self.panel = Panel::None;
    }

    fn world_dir(&self) -> PathBuf {
        let safe: String = self.world_name.text.trim().chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
        data_dir().join("worlds").join(if safe.is_empty() { "world".into() } else { safe })
    }

    fn seed_value(&self) -> u64 {
        let s = self.seed_text.text.trim();
        if s.is_empty() {
            return std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1);
        }
        s.parse::<u64>().unwrap_or_else(|_| s.bytes().fold(1469598103934665603u64, |h, b| (h ^ b as u64).wrapping_mul(1099511628211)))
    }

    /// Starts a server inside the game (127.0.0.1 only, or on the LAN) and joins it.
    pub fn host(&mut self, lan: bool) {
        self.leave();
        let port = if lan { cubeterra_common::DEFAULT_PORT } else { 0 };
        let cfg = cubeterra_server::Config {
            bind: if lan { format!("0.0.0.0:{port}") } else { "127.0.0.1:0".into() },
            world_dir: self.world_dir(),
            seed: self.seed_value(),
            motd: if lan { "LAN game".into() } else { "Single player".into() },
            max_view: 16,
            mobs: true,
        };
        match cubeterra_server::start(cfg) {
            Ok(server) => {
                let addr = server.addr.to_string();
                self.server = Some(server);
                self.lan = lan;
                self.connect_to(addr);
            }
            Err(e) => self.status = format!("Couldn't start the world: {e}"),
        }
    }

    pub fn join(&mut self) {
        self.leave();
        let addr = self.addr.text.trim().to_string();
        if addr.is_empty() {
            self.status = "Type a server address first".into();
            return;
        }
        self.lan = false;
        self.connect_to(addr);
    }

    /// Back to the title screen, closing any hosted world (it saves).
    pub fn leave(&mut self) {
        self.conn = None;
        if let Some(mut s) = self.server.take() {
            s.stop();
        }
        self.world.clear();
        self.others.clear();
        self.mobs.clear();
        self.screen = Screen::Title;
        self.panel = Panel::None;
        self.held.clear();
        self.look = Look::default();
        self.mining = None;
    }

    pub fn send(&self, msg: ToServer) {
        if let Some(c) = &self.conn {
            c.send(msg);
        }
    }

    pub fn say(&mut self, text: impl Into<String>) {
        self.chat.push(ChatLine { at: self.t, text: text.into(), system: true });
    }

    pub fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
        if mode == Mode::Survival {
            self.flying = false;
        }
        self.say(format!("Game mode: {}", if mode == Mode::Creative { "Creative" } else { "Survival" }));
    }

    pub fn submit_chat(&mut self) {
        let text = self.chat_edit.text.trim().to_string();
        self.chat_edit = TextEdit::new("");
        self.panel = Panel::None;
        if text.is_empty() {
            return;
        }
        match text.as_str() {
            "/gamemode creative" | "/gm c" | "/creative" => self.set_mode(Mode::Creative),
            "/gamemode survival" | "/gm s" | "/survival" => self.set_mode(Mode::Survival),
            "/fly" => {
                if self.mode == Mode::Creative {
                    self.flying = !self.flying;
                }
            }
            "/help" => {
                self.say("Client: /gamemode creative|survival, /fly, /kill");
                self.send(ToServer::Chat(text));
            }
            "/kill" => self.health = 0.0,
            _ => self.send(ToServer::Chat(text)),
        }
    }

    pub fn travel(&mut self, pos: Vec3) {
        self.send(ToServer::Travel { pos: pos.to_array() });
        self.panel = Panel::None;
    }

    pub fn respawn(&mut self) {
        self.health = 20.0;
        self.hunger = 20.0;
        self.air = 10.0;
        if self.mode == Mode::Survival {
            self.inv = [None; INV];
        }
        self.panel = Panel::None;
        let spawn = self.spawn;
        self.travel(spawn);
    }

    // -----------------------------------------------------------------------------------------
    // Inventory

    pub fn held_item(&self) -> Option<Stack> {
        self.inv[self.selected]
    }

    pub fn count(&self, id: BlockId) -> u32 {
        self.inv.iter().flatten().filter(|s| s.id == id).map(|s| s.n as u32).sum()
    }

    pub fn give(&mut self, id: BlockId, mut n: u8) {
        let max = block::max_stack(id);
        for s in self.inv.iter_mut().flatten() {
            if s.id == id && s.n < max {
                let take = n.min(max - s.n);
                s.n += take;
                n -= take;
                if n == 0 {
                    return;
                }
            }
        }
        while n > 0 {
            let Some(slot) = self.inv.iter_mut().find(|s| s.is_none()) else { return };
            let take = n.min(max);
            *slot = Some(Stack { id, n: take });
            n -= take;
        }
    }

    pub fn take(&mut self, id: BlockId, mut n: u32) {
        for slot in self.inv.iter_mut().rev() {
            if let Some(s) = slot
                && s.id == id
            {
                let k = (s.n as u32).min(n);
                s.n -= k as u8;
                n -= k;
                if s.n == 0 {
                    *slot = None;
                }
                if n == 0 {
                    return;
                }
            }
        }
    }

    fn use_held(&mut self) {
        if self.mode == Mode::Creative {
            return;
        }
        if let Some(s) = &mut self.inv[self.selected] {
            s.n -= 1;
            if s.n == 0 {
                self.inv[self.selected] = None;
            }
        }
    }

    /// Stations within reach.
    pub fn near_station(&self, st: Station) -> bool {
        let Some(want) = st.block() else { return true };
        if self.mode == Mode::Creative {
            return true;
        }
        let p = self.body.pos.floor().as_ivec3();
        for dy in -2..=3 {
            for dz in -4..=4 {
                for dx in -4..=4 {
                    if self.world.get(p.x + dx, p.y + dy, p.z + dz) == want {
                        return true;
                    }
                }
            }
        }
        false
    }

    pub fn can_craft(&self, r: &Recipe) -> bool {
        r.inputs.iter().all(|&(id, n)| self.count(id) >= n as u32) && self.near_station(r.station)
    }

    pub fn craft(&mut self, i: usize) {
        let r = &RECIPES[i];
        if !self.can_craft(r) {
            return;
        }
        for &(id, n) in r.inputs {
            self.take(id, n as u32);
        }
        self.give(r.out, r.count);
    }

    /// Clicking an inventory slot swaps it with the selected hotbar slot.
    pub fn click_slot(&mut self, i: usize) {
        if i < 9 {
            self.selected = i;
        } else {
            self.inv.swap(i, self.selected);
        }
    }

    pub fn pick_creative(&mut self, id: BlockId) {
        let n = block::max_stack(id);
        // Use an empty hotbar slot if the selected one is taken by something else.
        if self.inv[self.selected].is_some_and(|s| s.id != id)
            && let Some(free) = (0..9).find(|&i| self.inv[i].is_none())
        {
            self.selected = free;
        }
        self.inv[self.selected] = Some(Stack { id, n });
    }

    // -----------------------------------------------------------------------------------------
    // Camera

    pub fn forward(&self) -> Vec3 {
        Vec3::new(self.pitch.cos() * self.yaw.sin(), self.pitch.sin(), -self.pitch.cos() * self.yaw.cos())
    }

    pub fn eye(&self) -> Vec3 {
        let mut e = self.body.eye();
        if self.shake > 0.0 {
            let k = self.shake * 0.15;
            e += Vec3::new((self.t * 61.0).sin() as f32 * k, (self.t * 53.0).cos() as f32 * k, 0.0);
        }
        e
    }

    pub fn camera(&self) -> Mat4 {
        let (w, h) = (self.screen_px.0.max(1) as f32, self.screen_px.1.max(1) as f32);
        let mut fov = self.fov;
        if self.sprint && self.body.vel.with_y(0.0).length() > physics::WALK + 0.3 {
            fov += 6.0;
        }
        let proj = Mat4::perspective_rh(fov.to_radians(), w / h, 0.08, 1200.0);
        let eye = self.eye();
        let view = Mat4::look_to_rh(eye, self.forward(), Vec3::Y);
        proj * view
    }

    /// Where a world point lands on screen, in points, if in front of the camera.
    pub fn project(&self, p: Vec3, w: f32, h: f32) -> Option<(f32, f32)> {
        let c = self.view_proj * p.extend(1.0);
        if c.w <= 0.05 {
            return None;
        }
        let (x, y) = (c.x / c.w, c.y / c.w);
        if x.abs() > 1.2 || y.abs() > 1.2 {
            return None;
        }
        Some(((x * 0.5 + 0.5) * w, (0.5 - y * 0.5) * h))
    }

    pub fn daylight(&self) -> f32 {
        let sun = (self.time_of_day * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2).sin();
        (sun * 2.5 + 0.55).clamp(0.12, 1.0)
    }

    // -----------------------------------------------------------------------------------------
    // Input

    pub fn key_down(&mut self, k: &str) {
        self.held.insert(k.to_string());
    }

    pub fn key_up(&mut self, k: &str) {
        self.held.remove(k);
    }

    fn key(&self, k: &str) -> bool {
        self.held.contains(k)
    }

    pub fn playing(&self) -> bool {
        self.screen == Screen::Playing && self.panel == Panel::None
    }

    pub fn jump_pressed(&mut self) {
        if self.mode == Mode::Creative {
            if self.t - self.space_tap < 0.3 {
                self.flying = !self.flying;
                self.space_tap = -1.0;
            } else {
                self.space_tap = self.t;
            }
        }
    }

    pub fn drag(&mut self, phase: rune::Phase, dx: f32, dy: f32) {
        match phase {
            rune::Phase::Down => {
                self.look = Look { down: true, dragging: false, last: (0.0, 0.0) };
                self.mine_click();
            }
            rune::Phase::Move => {
                if !self.look.dragging && dx.hypot(dy) > 6.0 {
                    self.look.dragging = true;
                    self.mining = None;
                }
                let (ddx, ddy) = (dx - self.look.last.0, dy - self.look.last.1);
                self.look.last = (dx, dy);
                if self.look.dragging {
                    self.turn(ddx * 0.0055 * self.sensitivity, -ddy * 0.0055 * self.sensitivity);
                }
            }
            rune::Phase::Up => {
                self.look.down = false;
                self.look.dragging = false;
                self.mining = None;
            }
        }
    }

    pub fn turn(&mut self, dyaw: f32, dpitch: f32) {
        self.yaw = (self.yaw + dyaw).rem_euclid(std::f32::consts::TAU);
        self.pitch = (self.pitch + dpitch).clamp(-1.55, 1.55);
    }

    pub fn wheel(&mut self, dy: f32) {
        self.wheel_acc += dy;
        while self.wheel_acc.abs() >= 24.0 {
            let step = if self.wheel_acc > 0.0 { 8 } else { 1 };
            self.selected = (self.selected + step) % 9;
            self.wheel_acc -= 24.0 * self.wheel_acc.signum();
        }
    }

    /// A press: hit a mob, or start breaking a block.
    fn mine_click(&mut self) {
        if !self.playing() {
            return;
        }
        self.swing = 1.0;
        let eye = self.eye();
        let dir = self.forward();
        let reach = self.target.map_or(4.5, |h| h.dist);
        let mob = self
            .mobs
            .iter()
            .filter_map(|(id, m)| {
                let c = m.pos + Vec3::Y * 0.7;
                let t = (c - eye).dot(dir);
                if t < 0.0 || t > reach.min(4.5) {
                    return None;
                }
                ((eye + dir * t).distance(c) < 0.75).then_some((t, *id))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((_, id)) = mob {
            let dmg = block::melee_damage(self.held_item().map_or(0, |s| s.id));
            self.send(ToServer::Attack { mob: id, damage: dmg });
            return;
        }
        if self.mode == Mode::Creative {
            self.break_target();
            self.break_cooldown = 0.3;
        }
    }

    fn break_target(&mut self) {
        let Some(hit) = self.target else { return };
        let b = hit.block;
        if block::def(b).hardness < 0.0 && self.mode == Mode::Survival {
            return;
        }
        let p = hit.pos;
        self.world.set(p.x, p.y, p.z, block::AIR);
        self.send(ToServer::SetBlock { pos: p.to_array(), block: block::AIR });
        if self.mode == Mode::Survival {
            let held = self.held_item().map_or(0, |s| s.id);
            let d = block::def(b);
            if d.drop != block::AIR && block::can_harvest(held, b) {
                self.give(d.drop, 1);
            }
            let leaves = matches!(b, block::OAK_LEAVES | block::BIRCH_LEAVES | block::CHERRY_LEAVES | block::SPRUCE_LEAVES);
            if leaves && cubeterra_common::noise::unit(cubeterra_common::noise::hash3(self.t.to_bits(), p.x, p.y, p.z)) < 0.08 {
                self.give(block::APPLE, 1);
            }
            if b == block::TALL_GRASS && cubeterra_common::noise::unit(cubeterra_common::noise::hash3(7, p.x, p.y, p.z)) < 0.5 {
                self.give(block::TALL_GRASS, 1);
            }
            self.hunger = (self.hunger - 0.02).max(0.0);
        }
        // Plants on top of a broken block fall too.
        let above = self.world.get(p.x, p.y + 1, p.z);
        if block::def(above).kind == block::Kind::Plant {
            self.world.set(p.x, p.y + 1, p.z, block::AIR);
            self.send(ToServer::SetBlock { pos: [p.x, p.y + 1, p.z], block: block::AIR });
        }
    }

    /// Right click: use or place.
    pub fn use_click(&mut self) {
        if !self.playing() {
            return;
        }
        self.swing = 1.0;
        let held = self.held_item();
        if let Some(s) = held
            && let Some(food) = block::food_value(s.id)
            && self.hunger < 20.0
        {
            self.hunger = (self.hunger + food).min(20.0);
            self.use_held();
            return;
        }
        let Some(hit) = self.target else { return };
        match hit.block {
            block::TNT => {
                self.send(ToServer::Ignite { pos: hit.pos.to_array() });
                return;
            }
            block::CRAFTING_TABLE | block::FURNACE if !self.shift => {
                self.panel = Panel::Inventory;
                return;
            }
            _ => {}
        }
        let Some(s) = held else { return };
        if !block::is_block(s.id) {
            return;
        }
        let replace = matches!(block::def(hit.block).kind, block::Kind::Plant);
        let p = if replace { hit.pos } else { hit.pos + hit.normal };
        if !(0..HEIGHT).contains(&p.y) {
            return;
        }
        let cur = self.world.get(p.x, p.y, p.z);
        if cur != block::AIR && !block::is_liquid(cur) && block::def(cur).kind != block::Kind::Plant {
            return;
        }
        if block::is_solid(s.id) && self.body.overlaps_block(p) {
            return;
        }
        if block::is_solid(s.id) && self.others.values().any(|o| Body::new(o.pos).overlaps_block(p)) {
            return;
        }
        self.world.set(p.x, p.y, p.z, s.id);
        self.send(ToServer::SetBlock { pos: p.to_array(), block: s.id });
        self.use_held();
    }

    // -----------------------------------------------------------------------------------------
    // Network

    fn handle_net(&mut self) {
        let Some(conn) = &self.conn else { return };
        let mut events = Vec::new();
        while let Ok(ev) = conn.rx.try_recv() {
            events.push(ev);
            if events.len() > 400 {
                break;
            }
        }
        for ev in events {
            match ev {
                NetEvent::Connected => self.status = "Joining…".into(),
                NetEvent::Closed(why) => {
                    let msg = why;
                    self.leave();
                    self.status = msg;
                    return;
                }
                NetEvent::Msg(m) => self.message(m),
            }
        }
    }

    fn message(&mut self, m: ToClient) {
        match m {
            ToClient::Welcome { id, seed, spawn, time, world, motd } => {
                self.my_id = id;
                self.seed = seed;
                self.time_of_day = time;
                self.world_label = world;
                self.motd = motd.clone();
                self.spawn = Vec3::from(spawn);
                self.body = Body::new(Vec3::from(spawn));
                self.got_welcome = true;
                self.screen = Screen::Playing;
                self.status.clear();
                self.health = 20.0;
                self.hunger = 20.0;
                self.hint_until = self.t + 12.0;
                self.say(motd);
                self.say("Drag to look, click to mine, right-click to build. E: inventory, M: travel to cities, T: chat.");
            }
            ToClient::Column { cx, cz, data } => {
                if let Some(col) = Column::decode(&data) {
                    self.world.insert(cx, cz, col);
                }
            }
            ToClient::Unload { cx, cz } => self.world.remove(cx, cz),
            ToClient::BlockChanged { pos, block } => self.world.set(pos[0], pos[1], pos[2], block),
            ToClient::PlayerJoined { id, name, skin, pose } => {
                if id != self.my_id {
                    let pos = Vec3::from(pose.pos);
                    self.others.insert(id, Other { name, skin, target: pose, pos, yaw: pose.yaw, pitch: pose.pitch, walk: 0.0, speed: 0.0 });
                }
            }
            ToClient::PlayerLeft { id } => {
                self.others.remove(&id);
            }
            ToClient::PlayerMoved { id, pose } => {
                if let Some(o) = self.others.get_mut(&id) {
                    o.target = pose;
                }
            }
            ToClient::Chat { from, text } => {
                let system = from.is_none();
                let line = match from {
                    Some(f) => format!("<{f}> {text}"),
                    None => text,
                };
                self.chat.push(ChatLine { at: self.t, text: line, system });
                if self.chat.len() > 200 {
                    self.chat.remove(0);
                }
            }
            ToClient::Time(t) => self.time_of_day = t,
            ToClient::Teleport { pos } => {
                self.body = Body::new(Vec3::from(pos));
                self.mining = None;
            }
            ToClient::Mobs(list) => {
                let seen: HashSet<u32> = list.iter().map(|m| m.id).collect();
                self.mobs.retain(|id, _| seen.contains(id));
                for m in list {
                    let p = Vec3::from(m.pos);
                    let e = self.mobs.entry(m.id).or_insert(MobView { kind: m.kind, target: p, pos: p, yaw: m.yaw, walk: 0.0, speed: 0.0, hurt: false });
                    e.target = p;
                    e.yaw = m.yaw;
                    e.hurt = m.hurt;
                }
            }
            ToClient::Damage { amount, from } => {
                self.hurt(amount);
                let push = (self.body.pos - Vec3::from(from)).with_y(0.0).normalize_or_zero();
                self.body.vel += push * 6.0 + Vec3::Y * 4.0;
            }
            ToClient::Explosion { pos } => {
                let p = Vec3::from(pos);
                self.explosions.push((p, self.t));
                let d = p.distance(self.body.pos);
                self.shake = self.shake.max((1.0 - d / 30.0).max(0.0));
            }
            ToClient::Kicked(why) => {
                self.leave();
                self.status = why;
            }
        }
    }

    pub fn hurt(&mut self, amount: f32) {
        if self.mode == Mode::Creative || amount <= 0.0 || self.panel == Panel::Dead {
            return;
        }
        self.health -= amount;
        self.hurt_flash = 1.0;
        self.shake = self.shake.max(0.3);
    }

    // -----------------------------------------------------------------------------------------
    // The frame

    /// Called every frame. True when the view should be rebuilt.
    pub fn update(&mut self, t: f64) -> bool {
        let dt = ((t - self.last_t) as f32).clamp(0.0, 0.1);
        self.last_t = t;
        self.t = t;
        self.frame_dt = dt;
        if dt > 0.0 {
            self.fps = self.fps * 0.95 + (1.0 / dt) * 0.05;
        }
        self.handle_net();
        if self.screen != Screen::Playing {
            return true;
        }
        self.time_of_day = (self.time_of_day + dt / cubeterra_common::DAY_LENGTH).fract();
        self.hurt_flash = (self.hurt_flash - dt * 2.0).max(0.0);
        self.shake = (self.shake - dt * 1.5).max(0.0);
        self.swing = (self.swing - dt * 4.0).max(0.0);
        self.break_cooldown = (self.break_cooldown - dt).max(0.0);
        self.explosions.retain(|e| t - e.1 < 1.0);

        let feet = self.body.pos.floor().as_ivec3();
        let ready = self.world.loaded(feet.x, feet.z);
        if ready {
            self.step_player(dt);
        }
        self.update_target();
        if ready {
            self.step_mining(dt);
            self.step_survival(dt);
        }
        self.send_pose();
        self.animate_others(dt);
        let c = self.body.pos.floor().as_ivec3();
        self.world.pump((c.x.div_euclid(CHUNK), c.y.div_euclid(CHUNK), c.z.div_euclid(CHUNK)));
        self.view_proj = self.camera();
        true
    }

    fn step_player(&mut self, dt: f32) {
        let playing = self.playing();
        let k = |s: &State, key: &str| playing && s.key(key);
        if playing {
            let turn = 2.4 * dt;
            if self.key("arrowleft") {
                self.turn(-turn, 0.0);
            }
            if self.key("arrowright") {
                self.turn(turn, 0.0);
            }
            if self.key("arrowup") {
                self.turn(0.0, turn);
            }
            if self.key("arrowdown") {
                self.turn(0.0, -turn);
            }
        }
        let fwd = Vec3::new(self.yaw.sin(), 0.0, -self.yaw.cos());
        let right = Vec3::new(self.yaw.cos(), 0.0, self.yaw.sin());
        let mut wish = Vec3::ZERO;
        if k(self, "w") {
            wish += fwd;
        }
        if k(self, "s") {
            wish -= fwd;
        }
        if k(self, "d") {
            wish += right;
        }
        if k(self, "a") {
            wish -= right;
        }
        let moving = wish.length_squared() > 0.0;
        if !moving || !k(self, "w") {
            self.sprint = false;
        } else if self.ctrl || k(self, "r") {
            self.sprint = true;
        }
        let sneak = playing && self.shift && !self.flying;
        let speed = if self.flying {
            physics::FLY * if self.sprint { 2.2 } else { 1.0 }
        } else if sneak {
            1.4
        } else if self.sprint && self.hunger > 6.0 {
            physics::SPRINT
        } else {
            physics::WALK
        };
        let wish = wish.normalize_or_zero() * speed;
        let input = physics::Input { wish, jump: k(self, " "), descend: playing && self.shift, flying: self.flying };
        let world = &self.world;
        let get = |x: i32, y: i32, z: i32| world.get_solid(x, y, z);
        let before = self.body;
        let damage = self.body.step(dt, &input, &get);
        // Sneaking keeps you from walking off edges.
        if sneak && before.on_ground && !self.body.on_ground && self.body.vel.y <= 0.0 {
            let support = |p: Vec3| {
                [(-0.3, -0.3), (0.3, -0.3), (-0.3, 0.3), (0.3, 0.3)]
                    .iter()
                    .any(|(dx, dz)| block::is_solid(world.get_solid((p.x + dx).floor() as i32, (p.y - 0.5).floor() as i32, (p.z + dz).floor() as i32)))
            };
            if !support(self.body.pos) {
                self.body = before;
                self.body.vel.x = 0.0;
                self.body.vel.z = 0.0;
            }
        }
        if self.flying && self.body.on_ground && self.mode == Mode::Creative && !k(self, " ") {
            self.flying = false;
        }
        if self.mode == Mode::Survival {
            self.hurt(damage);
        }
        if self.body.pos.y < -20.0 {
            self.hurt(4.0);
            if self.mode == Mode::Creative {
                let spawn = self.spawn;
                self.travel(spawn);
            }
        }
    }

    fn update_target(&mut self) {
        let reach = if self.mode == Mode::Creative { 7.0 } else { 5.0 };
        let world = &self.world;
        let get = |x: i32, y: i32, z: i32| world.get(x, y, z);
        self.target = if self.screen == Screen::Playing { raycast(self.eye(), self.forward(), reach, &get) } else { None };
    }

    fn step_mining(&mut self, dt: f32) {
        if !self.look.down || self.look.dragging || !self.playing() {
            self.mining = None;
            return;
        }
        if self.mode == Mode::Creative {
            if self.break_cooldown <= 0.0 && self.target.is_some() {
                self.break_target();
                self.break_cooldown = 0.25;
            }
            return;
        }
        let Some(hit) = self.target else {
            self.mining = None;
            return;
        };
        let hardness = block::def(hit.block).hardness;
        if hardness < 0.0 {
            self.mining = None;
            return;
        }
        let held = self.held_item().map_or(0, |s| s.id);
        let mut speed = block::tool_speed(held, hit.block);
        if !block::can_harvest(held, hit.block) {
            speed *= 0.3;
        }
        if self.body.in_water && !self.body.on_ground {
            speed *= 0.2;
        }
        let time = (hardness / speed).max(0.05);
        let progress = match self.mining {
            Some((p, prog)) if p == hit.pos => prog + dt / time,
            _ => dt / time,
        };
        self.swing = self.swing.max(0.6);
        if progress >= 1.0 {
            self.break_target();
            self.mining = None;
        } else {
            self.mining = Some((hit.pos, progress));
        }
    }

    fn step_survival(&mut self, dt: f32) {
        if self.mode == Mode::Creative {
            self.health = 20.0;
            self.hunger = 20.0;
            self.air = 10.0;
            return;
        }
        if self.panel == Panel::Dead {
            return;
        }
        let moving = self.body.vel.with_y(0.0).length();
        self.hunger = (self.hunger - dt * (0.012 + if self.sprint { 0.05 } else { 0.0 } + moving * 0.002)).max(0.0);
        if self.body.head_in_water {
            self.air -= dt;
            if self.air < 0.0 {
                self.air = 1.0;
                self.hurt(2.0);
            }
        } else {
            self.air = (self.air + dt * 3.0).min(10.0);
        }
        if self.body.in_lava {
            self.regen += dt;
            if self.regen > 0.5 {
                self.regen = 0.0;
                self.hurt(4.0);
            }
        }
        if self.hunger > 17.0 && self.health < 20.0 {
            self.regen += dt;
            if self.regen > 2.0 {
                self.regen = 0.0;
                self.health = (self.health + 1.0).min(20.0);
                self.hunger -= 0.3;
            }
        } else if self.hunger <= 0.0 {
            self.regen += dt;
            if self.regen > 4.0 {
                self.regen = 0.0;
                self.hurt(1.0);
            }
        }
        if self.health <= 0.0 {
            self.health = 0.0;
            self.panel = Panel::Dead;
            self.held.clear();
            self.look = Look::default();
        }
    }

    fn send_pose(&mut self) {
        let pose = Pose { pos: self.body.pos.to_array(), yaw: self.yaw, pitch: self.pitch };
        let (last, at) = self.last_move;
        let moved = Vec3::from(pose.pos).distance(Vec3::from(last.pos)) > 0.01 || (pose.yaw - last.yaw).abs() > 0.01 || (pose.pitch - last.pitch).abs() > 0.01;
        if (moved && self.t - at > 0.05) || self.t - at > 1.0 {
            self.send(ToServer::Move(pose));
            self.last_move = (pose, self.t);
        }
    }

    fn animate_others(&mut self, dt: f32) {
        let k = (dt * 12.0).min(1.0);
        for o in self.others.values_mut() {
            let target = Vec3::from(o.target.pos);
            let before = o.pos;
            if o.pos.distance(target) > 8.0 {
                o.pos = target;
            } else {
                o.pos += (target - o.pos) * k;
            }
            let mut dy = o.target.yaw - o.yaw;
            dy = (dy + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
            o.yaw += dy * k;
            o.pitch += (o.target.pitch - o.pitch) * k;
            let v = (o.pos - before).with_y(0.0).length() / dt.max(1e-3);
            o.speed += (v - o.speed) * k;
            o.walk += o.speed * dt * 1.6;
        }
        for m in self.mobs.values_mut() {
            let before = m.pos;
            if m.pos.distance(m.target) > 6.0 {
                m.pos = m.target;
            } else {
                m.pos += (m.target - m.pos) * k;
            }
            let v = (m.pos - before).with_y(0.0).length() / dt.max(1e-3);
            m.speed += (v - m.speed) * k;
            m.walk += m.speed * dt * 2.0;
        }
    }

    /// Nearest city, its distance in blocks, and its compass direction.
    pub fn nearest_city(&self) -> Option<(&'static str, f32, &'static str)> {
        let p = self.body.pos;
        CITIES
            .iter()
            .map(|c| {
                let d = Vec3::new(c.x as f32 - p.x, 0.0, c.z as f32 - p.z);
                (c, d)
            })
            .min_by(|a, b| a.1.length().total_cmp(&b.1.length()))
            .map(|(c, d)| {
                let ang = d.x.atan2(-d.z).to_degrees().rem_euclid(360.0);
                let dirs = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];
                (c.name, d.length(), dirs[(((ang + 22.5) / 45.0) as usize) % 8])
            })
    }

    pub fn biome_here(&self) -> &'static str {
        let p = self.body.pos.floor().as_ivec3();
        cubeterra_common::worldgen::sample(self.seed, p.x, p.z).biome.name()
    }
}

impl Default for State {
    fn default() -> Self {
        State::new()
    }
}
