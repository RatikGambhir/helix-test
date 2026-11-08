//! Game state and the per-frame update: networking, movement, mining, building, inventory,
//! crafting, survival and weather.

use crate::audio::{Audio, Sound};
use crate::net::{self, Connection, NetEvent};
use crate::textures::{self, Atlas};
use crate::world::World;
use glam::{IVec3, Mat4, Vec3};
use rune::{Pixels, TextEdit};
use runecraft_common::block::{self, BlockId, Kind, Stack};
use runecraft_common::chunk::{CHUNK, Column, HEIGHT};
use runecraft_common::city::CITIES;
use runecraft_common::craft::{self, Recipe};
use runecraft_common::noise::{hash3, unit};
use runecraft_common::physics::{self, Body, Hit, raycast, raycast_fluids};
use runecraft_common::protocol::*;
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
    /// A crafting table's 3×3 grid.
    Crafting,
    /// A chest or furnace (see `State::container`).
    Container,
    Map,
    Chat,
    Dead,
    Help,
    Sleeping,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Survival,
    Creative,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum View {
    First,
    Back,
    Front,
}

/// Anywhere an item can sit in a menu.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Slot {
    Inv(usize),
    Armor(usize),
    Grid(usize),
    Output,
    Box(usize),
}

pub struct ContainerView {
    pub pos: [i32; 3],
    pub kind: ContainerKind,
    pub slots: Vec<Option<Stack>>,
    pub burn: f32,
    pub progress: f32,
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
    pub held: u8,
}

/// A dropped item stack, as the server last reported it.
pub struct ItemView {
    pub item: u8,
    pub target: Vec3,
    pub pos: Vec3,
}

pub struct Particle {
    pub pos: Vec3,
    pub vel: Vec3,
    pub tile: u8,
    pub life: f32,
    pub size: f32,
    pub gravity: f32,
    pub glow: bool,
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
    pub search: TextEdit,
    pub status: String,
    pub server: Option<runecraft_server::Server>,
    pub lan: bool,
    pub conn: Option<Connection>,
    pub waker: Option<rune::Waker>,
    pub audio: Audio,
    pub saved_worlds: Vec<String>,

    pub world: World,
    pub atlas: Arc<Atlas>,
    pub icons: HashMap<BlockId, Pixels>,
    pub map: Pixels,
    pub minimap: Pixels,
    pub map_at: f64,
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
    pub view: View,

    pub inv: [Option<Stack>; INV],
    pub armor: [Option<Stack>; 4],
    pub grid: [Option<Stack>; 9],
    pub cursor: Option<Stack>,
    pub container: Option<ContainerView>,
    pub selected: usize,
    pub held: HashSet<String>,
    pub shift: bool,
    pub ctrl: bool,
    pub look: Look,
    pub mining: Option<(IVec3, f32)>,
    pub dig_sound: f32,
    pub swing: f32,
    pub break_cooldown: f32,
    pub use_cooldown: f32,
    pub target: Option<Hit>,
    pub others: HashMap<PlayerId, Other>,
    pub items: HashMap<u32, ItemView>,
    pub particles: Vec<Particle>,
    pub chat: Vec<ChatLine>,
    pub time_of_day: f32,
    pub rain: f32,
    pub rain_target: f32,
    pub thunder: bool,
    pub lightning: Vec<(Vec3, f64)>,
    pub t: f64,
    pub last_t: f64,
    pub frame_dt: f32,
    pub fps: f32,
    pub screen_px: (u32, u32),
    pub view_proj: Mat4,
    pub show_debug: bool,
    pub show_minimap: bool,
    pub view_distance: u8,
    pub fov: f32,
    pub sensitivity: f32,
    pub capture_enabled: bool,
    pub captured: bool,
    pub pointer: Option<(f32, f32)>,
    pub window_size: (f32, f32),
    pub ui_scale: f32,
    pub sound_on: bool,
    pub music_on: bool,
    pub last_move: (Pose, f64),
    pub last_save: f64,
    pub last_held: u8,
    pub space_tap: f64,
    pub step_dist: f32,
    pub explosions: Vec<(Vec3, f64)>,
    pub skin: u8,
    pub hint_until: f64,
    pub wheel_acc: f32,
    pub selected_at: f64,
}

fn data_dir() -> PathBuf {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"));
    match home {
        Some(h) => PathBuf::from(h).join(".runecraft"),
        None => PathBuf::from("runecraft-data"),
    }
}

pub fn list_worlds() -> Vec<String> {
    let mut v: Vec<(std::time::SystemTime, String)> = std::fs::read_dir(data_dir().join("worlds"))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().join("world.dat").exists())
        .map(|e| {
            let t = e.path().join("world.dat").metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH);
            (t, e.file_name().to_string_lossy().into_owned())
        })
        .collect();
    v.sort_by(|a, b| b.0.cmp(&a.0));
    v.into_iter().map(|(_, n)| n).filter(|n| n != "snapshot-world").collect()
}

fn starter(mode: Mode) -> [Option<Stack>; INV] {
    let mut inv = [None; INV];
    if mode == Mode::Creative {
        let picks = [block::GRASS, block::STONE, block::PLANKS, block::BRICK, block::GLASS, block::OAK_LOG, block::TORCH, block::WINDOW_BLUE, block::DOOR];
        for (i, b) in picks.iter().enumerate() {
            inv[i] = Some(Stack::new(*b, block::max_stack(*b)));
        }
    }
    inv
}

/// A block's sound family: 0 stone, 1 wood, 2 soil, 3 glass, 4 sand/snow, 5 plant.
pub fn material_sound(b: BlockId) -> u8 {
    match b {
        block::GLASS | block::ICE | block::WINDOW_BLUE | block::WINDOW_DARK | block::WINDOW_WARM | block::GLOWSTONE => 3,
        block::SAND | block::SNOW | block::GRAVEL => 4,
        block::DIRT | block::GRASS | block::CLAY | block::FARMLAND => 2,
        block::OAK_LOG | block::BIRCH_LOG | block::SPRUCE_LOG | block::PLANKS | block::CRAFTING_TABLE | block::BOOKSHELF | block::CHEST | block::BED => 1,
        _ if block::door_parts(b).is_some() || block::is_ladder(b) => 1,
        _ if matches!(block::def(b).kind, Kind::Plant | Kind::Cutout) => 5,
        _ if (block::WOOL_WHITE..=block::WOOL_BLACK).contains(&b) => 2,
        _ => 0,
    }
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
        let user = std::env::var("USER").or_else(|_| std::env::var("USERNAME")).unwrap_or_else(|_| "Player".into());
        let name: String = user.chars().filter(|c| c.is_alphanumeric()).take(16).collect();
        let skin = (runecraft_common::noise::hash(name.bytes().map(|b| b as u64).sum::<u64>()) % 32) as u8;
        State {
            screen: Screen::Title,
            panel: Panel::None,
            name: TextEdit::new(if name.is_empty() { "Player".to_string() } else { name }),
            addr: TextEdit::new("127.0.0.1"),
            world_name: TextEdit::new("My World"),
            seed_text: TextEdit::new(""),
            chat_edit: TextEdit::new(""),
            search: TextEdit::new(""),
            status: String::new(),
            server: None,
            lan: false,
            conn: None,
            waker: None,
            audio: Audio::new(),
            saved_worlds: list_worlds(),
            world: World::new(),
            atlas,
            icons,
            map: Pixels::new(),
            minimap: Pixels::new(),
            map_at: -10.0,
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
            mode: Mode::Survival,
            sprint: false,
            health: 20.0,
            hunger: 20.0,
            air: 10.0,
            hurt_flash: 0.0,
            shake: 0.0,
            regen: 0.0,
            view: View::First,
            inv: starter(Mode::Survival),
            armor: [None; 4],
            grid: [None; 9],
            cursor: None,
            container: None,
            selected: 0,
            held: HashSet::new(),
            shift: false,
            ctrl: false,
            look: Look::default(),
            mining: None,
            dig_sound: 0.0,
            swing: 0.0,
            break_cooldown: 0.0,
            use_cooldown: 0.0,
            target: None,
            others: HashMap::new(),
            items: HashMap::new(),
            particles: Vec::new(),
            chat: Vec::new(),
            time_of_day: 0.3,
            rain: 0.0,
            rain_target: 0.0,
            thunder: false,
            lightning: Vec::new(),
            t: 0.0,
            last_t: 0.0,
            frame_dt: 0.016,
            fps: 60.0,
            screen_px: (1280, 800),
            view_proj: Mat4::IDENTITY,
            show_debug: false,
            show_minimap: true,
            view_distance: 8,
            fov: 75.0,
            sensitivity: 1.0,
            capture_enabled: crate::capture::supported(),
            captured: false,
            pointer: None,
            window_size: (1280.0, 800.0),
            ui_scale: 1.0,
            sound_on: true,
            music_on: true,
            last_move: (Pose { pos: [0.0; 3], yaw: 0.0, pitch: 0.0 }, 0.0),
            last_save: 0.0,
            last_held: 0,
            space_tap: -1.0,
            step_dist: 0.0,
            explosions: Vec::new(),
            skin,
            hint_until: 0.0,
            wheel_acc: 0.0,
            selected_at: -10.0,
        }
    }

    pub fn sound(&self, s: Sound, at: Option<Vec3>) {
        if !self.sound_on {
            return;
        }
        let vol = match at {
            Some(p) => (1.0 - p.distance(self.body.pos) / 32.0).clamp(0.0, 1.0),
            None => 1.0,
        };
        self.audio.play(s, vol);
    }

    // -----------------------------------------------------------------------------------------
    // Starting and leaving games

    fn connect_to(&mut self, addr: String) {
        self.world.clear();
        self.others.clear();
        self.items.clear();
        self.got_welcome = false;
        self.status = format!("Connecting to {addr}…");
        self.conn = Some(net::connect(addr, self.name.text.trim().to_string(), self.view_distance, self.skin, self.waker.clone()));
        self.screen = Screen::Connecting;
        self.panel = Panel::None;
    }

    fn world_dir_for(name: &str) -> PathBuf {
        let safe: String = name.trim().chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' || c == ' ' { c } else { '_' }).collect();
        data_dir().join("worlds").join(if safe.trim().is_empty() { "world".into() } else { safe })
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
        let name = self.world_name.text.clone();
        self.host_world(&name, lan);
    }

    pub fn host_world(&mut self, world: &str, lan: bool) {
        self.leave();
        let port = if lan { runecraft_common::DEFAULT_PORT } else { 0 };
        let cfg = runecraft_server::Config {
            bind: if lan { format!("0.0.0.0:{port}") } else { "127.0.0.1:0".into() },
            world_dir: Self::world_dir_for(world),
            seed: self.seed_value(),
            motd: if lan { "LAN game".into() } else { "Single player".into() },
            max_view: 16,
        };
        match runecraft_server::start(cfg) {
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
        if self.got_welcome {
            self.close_panel();
            self.save_player();
        }
        self.conn = None;
        if let Some(mut s) = self.server.take() {
            s.stop();
        }
        self.world.clear();
        self.others.clear();
        self.items.clear();
        self.particles.clear();
        self.screen = Screen::Title;
        self.panel = Panel::None;
        self.held.clear();
        self.look = Look::default();
        self.mining = None;
        self.captured = false;
        self.got_welcome = false;
        self.saved_worlds = list_worlds();
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
        self.save_player();
    }

    fn save_player(&self) {
        self.send(ToServer::Save(PlayerSave {
            inv: self.inv.to_vec(),
            armor: self.armor.to_vec(),
            health: self.health,
            hunger: self.hunger,
            creative: self.mode == Mode::Creative,
        }));
    }

    pub fn submit_chat(&mut self) {
        let text = self.chat_edit.text.trim().to_string();
        self.chat_edit = TextEdit::new("");
        self.panel = Panel::None;
        if text.is_empty() {
            return;
        }
        match text.as_str() {
            "/gamemode creative" | "/gm c" | "/creative" | "/gamemode 1" => self.set_mode(Mode::Creative),
            "/gamemode survival" | "/gm s" | "/survival" | "/gamemode 0" => self.set_mode(Mode::Survival),
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
        self.panel = Panel::None;
        let spawn = self.spawn;
        self.travel(spawn);
        self.save_player();
    }

    // -----------------------------------------------------------------------------------------
    // Inventory

    pub fn held_item(&self) -> Option<Stack> {
        self.inv[self.selected]
    }

    /// Adds a stack to the inventory; returns what didn't fit.
    pub fn give(&mut self, stack: Stack) -> Option<Stack> {
        let mut n = stack.n;
        let max = block::max_stack(stack.id);
        for s in self.inv.iter_mut().flatten() {
            if s.same(&stack) && s.n < max {
                let take = n.min(max - s.n);
                s.n += take;
                n -= take;
                if n == 0 {
                    return None;
                }
            }
        }
        while n > 0 {
            let Some(slot) = self.inv.iter_mut().find(|s| s.is_none()) else { return Some(Stack { n, ..stack }) };
            let take = n.min(max);
            *slot = Some(Stack { n: take, ..stack });
            n -= take;
        }
        None
    }

    /// Gives, and throws on the ground whatever doesn't fit.
    pub fn give_or_drop(&mut self, stack: Stack) {
        if let Some(rest) = self.give(stack) {
            self.throw(rest, 2.0);
        }
    }

    pub fn throw(&mut self, stack: Stack, speed: f32) {
        let from = self.body.eye() - Vec3::Y * 0.3;
        let vel = self.forward() * speed + Vec3::Y * 1.5;
        self.send(ToServer::Drop { stack, pos: from.to_array(), vel: vel.to_array() });
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

    /// Wears the held tool down by one use.
    fn wear_held(&mut self) {
        if self.mode == Mode::Creative {
            return;
        }
        if let Some(s) = &mut self.inv[self.selected] {
            let max = block::max_durability(s.id);
            if max > 0 {
                s.dmg += 1;
                if s.dmg >= max {
                    self.inv[self.selected] = None;
                    self.sound(Sound::Break(0), None);
                    self.say("Your tool broke!");
                }
            }
        }
    }

    pub fn armor_points(&self) -> f32 {
        self.armor.iter().flatten().filter_map(|s| block::armor(s.id)).map(|a| a.1 as f32).sum()
    }

    pub fn grid_size(&self) -> usize {
        if self.panel == Panel::Crafting { 3 } else { 2 }
    }

    fn grid_index(&self, i: usize) -> usize {
        // The 2×2 grid uses the top-left corner of the 3×3 storage.
        let s = self.grid_size();
        (i / s) * 3 + i % s
    }

    pub fn grid_cell(&self, i: usize) -> Option<Stack> {
        self.grid[self.grid_index(i)]
    }

    pub fn recipe(&self) -> Option<&'static Recipe> {
        let s = self.grid_size();
        let cells: Vec<Option<BlockId>> = (0..s * s).map(|i| self.grid_cell(i).map(|st| st.id)).collect();
        craft::match_grid(&cells, s)
    }

    pub fn output(&self) -> Option<Stack> {
        self.recipe().map(|r| Stack::new(r.out, r.count))
    }

    pub fn slot(&self, s: Slot) -> Option<Stack> {
        match s {
            Slot::Inv(i) => self.inv[i],
            Slot::Armor(i) => self.armor[i],
            Slot::Grid(i) => self.grid_cell(i),
            Slot::Output => self.output(),
            Slot::Box(i) => self.container.as_ref().and_then(|c| c.slots.get(i).copied().flatten()),
        }
    }

    fn set_slot(&mut self, s: Slot, v: Option<Stack>) {
        let v = v.filter(|st| st.n > 0);
        match s {
            Slot::Inv(i) => self.inv[i] = v,
            Slot::Armor(i) => self.armor[i] = v,
            Slot::Grid(i) => {
                let gi = self.grid_index(i);
                self.grid[gi] = v;
            }
            Slot::Output => {}
            Slot::Box(i) => {
                if let Some(c) = &mut self.container
                    && i < c.slots.len()
                {
                    c.slots[i] = v;
                    let pos = c.pos;
                    self.send(ToServer::ContainerSet { pos, slot: i as u8, stack: v });
                }
            }
        }
    }

    fn accepts(&self, s: Slot, st: &Stack) -> bool {
        match s {
            Slot::Armor(i) => block::armor(st.id).is_some_and(|a| a.0 == i),
            Slot::Output => false,
            Slot::Box(2) if self.container.as_ref().is_some_and(|c| c.kind == ContainerKind::Furnace) => false,
            _ => true,
        }
    }

    /// One crafting: takes an item from every grid cell.
    fn consume_grid(&mut self) {
        let n = self.grid_size() * self.grid_size();
        for i in 0..n {
            let gi = self.grid_index(i);
            if let Some(s) = &mut self.grid[gi] {
                s.n -= 1;
                if s.n == 0 {
                    // Buckets come back empty.
                    self.grid[gi] = if matches!(s.id, block::WATER_BUCKET | block::LAVA_BUCKET) { Some(Stack::new(block::BUCKET, 1)) } else { None };
                }
            }
        }
    }

    /// A click on a menu slot, as the genre does it: pick up, put down, swap, split with the
    /// right button, and move straight across with shift.
    pub fn click(&mut self, s: Slot, right: bool) {
        self.sound(Sound::Click, None);
        if s == Slot::Output {
            let Some(out) = self.output() else { return };
            if self.shift {
                // Craft as many as possible straight into the inventory.
                for _ in 0..64 {
                    let Some(out) = self.output() else { break };
                    if self.give(out).is_some() {
                        break;
                    }
                    self.consume_grid();
                }
                return;
            }
            match self.cursor {
                None => self.cursor = Some(out),
                Some(c) if c.same(&out) && c.n as u32 + out.n as u32 <= block::max_stack(c.id) as u32 => self.cursor = Some(Stack { n: c.n + out.n, ..c }),
                _ => return,
            }
            self.consume_grid();
            return;
        }
        if self.shift && !right {
            self.quick_move(s);
            return;
        }
        let here = self.slot(s);
        match (self.cursor, here) {
            (None, None) => {}
            (None, Some(h)) => {
                if right {
                    let half = h.n.div_ceil(2);
                    self.cursor = Some(Stack { n: half, ..h });
                    self.set_slot(s, Some(Stack { n: h.n - half, ..h }));
                } else {
                    self.cursor = Some(h);
                    self.set_slot(s, None);
                }
            }
            (Some(c), None) => {
                if !self.accepts(s, &c) {
                    return;
                }
                if right {
                    self.set_slot(s, Some(c.one()));
                    self.cursor = if c.n > 1 { Some(Stack { n: c.n - 1, ..c }) } else { None };
                } else {
                    self.set_slot(s, Some(c));
                    self.cursor = None;
                }
            }
            (Some(c), Some(h)) => {
                if c.same(&h) {
                    let max = block::max_stack(h.id);
                    let room = max.saturating_sub(h.n);
                    let moved = if right { 1.min(room) } else { c.n.min(room) };
                    self.set_slot(s, Some(Stack { n: h.n + moved, ..h }));
                    self.cursor = if c.n > moved { Some(Stack { n: c.n - moved, ..c }) } else { None };
                } else if self.accepts(s, &c) {
                    self.set_slot(s, Some(c));
                    self.cursor = Some(h);
                }
            }
        }
    }

    fn quick_move(&mut self, s: Slot) {
        let Some(st) = self.slot(s) else { return };
        match s {
            Slot::Inv(i) => {
                // Armor goes on; into an open container; otherwise between hotbar and backpack.
                if let Some((a, _)) = block::armor(st.id)
                    && self.armor[a].is_none()
                    && self.panel == Panel::Inventory
                {
                    self.armor[a] = Some(st);
                    self.inv[i] = None;
                    return;
                }
                if let Some(c) = &self.container {
                    let targets: Vec<usize> = match c.kind {
                        ContainerKind::Chest => (0..c.slots.len()).collect(),
                        ContainerKind::Furnace => {
                            if block::fuel_time(st.id) > 0.0 && craft::smelt(st.id).is_none() { vec![1] } else { vec![0] }
                        }
                    };
                    let mut left = st;
                    for t in targets {
                        match self.slot(Slot::Box(t)) {
                            None => {
                                self.set_slot(Slot::Box(t), Some(left));
                                left.n = 0;
                            }
                            Some(cs) if cs.same(&left) => {
                                let room = block::max_stack(cs.id) - cs.n;
                                let m = room.min(left.n);
                                self.set_slot(Slot::Box(t), Some(Stack { n: cs.n + m, ..cs }));
                                left.n -= m;
                            }
                            _ => {}
                        }
                        if left.n == 0 {
                            break;
                        }
                    }
                    self.inv[i] = (left.n > 0).then_some(left);
                    return;
                }
                let range = if i < 9 { 9..INV } else { 0..9 };
                self.inv[i] = None;
                let mut left = st;
                for j in range.clone() {
                    if let Some(o) = &mut self.inv[j]
                        && o.same(&left)
                    {
                        let m = (block::max_stack(o.id) - o.n).min(left.n);
                        o.n += m;
                        left.n -= m;
                    }
                }
                if left.n > 0 {
                    match range.clone().find(|&j| self.inv[j].is_none()) {
                        Some(j) => self.inv[j] = Some(left),
                        None => self.inv[i] = Some(left),
                    }
                }
            }
            Slot::Armor(_) | Slot::Grid(_) | Slot::Box(_) => {
                self.set_slot(s, None);
                if let Some(rest) = self.give(st) {
                    self.set_slot(s, Some(rest));
                }
            }
            Slot::Output => {}
        }
    }

    /// Creative catalog: click takes one (or a full stack with shift); clicking the same item again adds more.
    pub fn pick_creative(&mut self, id: BlockId) {
        self.sound(Sound::Click, None);
        let max = block::max_stack(id);
        self.cursor = match self.cursor {
            Some(c) if c.id == id => Some(Stack { n: if self.shift { max } else { (c.n + 1).min(max) }, ..c }),
            _ => Some(Stack::new(id, if self.shift { max } else { 1 })),
        };
    }

    /// Recipe book: whether you hold everything a recipe needs (and have the table it needs).
    pub fn book_can(&self, r: &Recipe) -> bool {
        if !r.small() && self.panel != Panel::Crafting {
            return false;
        }
        if self.mode == Mode::Creative {
            return true;
        }
        let mut have: Vec<(BlockId, u32)> = Vec::new();
        for st in self.inv.iter().flatten() {
            match have.iter_mut().find(|h| h.0 == st.id) {
                Some(h) => h.1 += st.n as u32,
                None => have.push((st.id, st.n as u32)),
            }
        }
        r.ingredients().iter().all(|(want, n)| {
            let mut need = *n as u32;
            for h in have.iter_mut().filter(|h| want.accepts(h.0)) {
                let k = h.1.min(need);
                h.1 -= k;
                need -= k;
            }
            need == 0
        })
    }

    /// Recipe book: craft one straight from the inventory.
    pub fn book_craft(&mut self, i: usize) {
        let r = &craft::RECIPES[i];
        if !self.book_can(r) {
            return;
        }
        if self.mode == Mode::Survival {
            for (want, n) in r.ingredients() {
                let mut need = n as u32;
                for slot in self.inv.iter_mut().rev() {
                    if let Some(st) = slot
                        && want.accepts(st.id)
                        && need > 0
                    {
                        let k = (st.n as u32).min(need);
                        st.n -= k as u8;
                        need -= k;
                        if st.n == 0 {
                            *slot = None;
                        }
                    }
                }
            }
        }
        self.give_or_drop(Stack::new(r.out, r.count));
        self.sound(Sound::Pickup, None);
    }

    pub fn trash_cursor(&mut self) {
        self.cursor = None;
    }

    /// Closing a menu: what's in the crafting grid and on the cursor goes back to you.
    pub fn close_panel(&mut self) {
        let leftovers: Vec<Stack> = self.grid.iter_mut().filter_map(|s| s.take()).chain(self.cursor.take()).collect();
        for s in leftovers {
            self.give_or_drop(s);
        }
        if self.container.take().is_some() {
            self.send(ToServer::CloseContainer);
        }
        if matches!(self.panel, Panel::Inventory | Panel::Crafting | Panel::Container) {
            self.save_player();
        }
        self.panel = Panel::None;
    }

    pub fn open(&mut self, panel: Panel) {
        self.held.clear();
        self.look = Look::default();
        self.captured = false;
        self.panel = panel;
    }

    // -----------------------------------------------------------------------------------------
    // Camera

    pub fn forward(&self) -> Vec3 {
        Vec3::new(self.pitch.cos() * self.yaw.sin(), self.pitch.sin(), -self.pitch.cos() * self.yaw.cos())
    }

    pub fn eye(&self) -> Vec3 {
        let mut e = self.body.eye();
        if self.shift && !self.flying && self.body.on_ground {
            e.y -= 0.12;
        }
        if self.shake > 0.0 {
            let k = self.shake * 0.15;
            e += Vec3::new((self.t * 61.0).sin() as f32 * k, (self.t * 53.0).cos() as f32 * k, 0.0);
        }
        e
    }

    /// Where the camera actually is (behind or in front of you in third person).
    pub fn camera_pos(&self) -> (Vec3, Vec3) {
        let eye = self.eye();
        let fwd = self.forward();
        let (dir, look) = match self.view {
            View::First => return (eye, fwd),
            View::Back => (-fwd, fwd),
            View::Front => (fwd, -fwd),
        };
        let world = &self.world;
        let get = |x: i32, y: i32, z: i32| world.get(x, y, z);
        let dist = raycast(eye, dir, 4.0, &get).map_or(4.0, |h| (h.dist - 0.3).max(0.3));
        (eye + dir * dist, look)
    }

    pub fn camera(&self) -> Mat4 {
        let (w, h) = (self.screen_px.0.max(1) as f32, self.screen_px.1.max(1) as f32);
        let mut fov = self.fov;
        if self.sprint && self.body.vel.with_y(0.0).length() > physics::WALK + 0.3 {
            fov += 8.0;
        }
        let proj = Mat4::perspective_rh(fov.to_radians(), w / h, 0.06, 1200.0);
        let (pos, look) = self.camera_pos();
        proj * Mat4::look_to_rh(pos, look, Vec3::Y)
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
        (sun * 2.5 + 0.55).clamp(0.12, 1.0) * (1.0 - self.rain * 0.35)
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

    pub fn select(&mut self, i: usize) {
        self.selected = i % 9;
        self.selected_at = self.t;
    }

    /// Whether moving the mouse turns the camera without a button held.
    pub fn mouse_look(&self) -> bool {
        self.capture_enabled && self.captured && self.playing()
    }

    /// The pointer moved to (x, y) window points.
    pub fn pointer_at(&mut self, x: f32, y: f32) {
        if !self.mouse_look() {
            self.pointer = Some((x, y));
            return;
        }
        if let Some((px, py)) = self.pointer {
            self.turn((x - px) * 0.0045 * self.sensitivity, -(y - py) * 0.0045 * self.sensitivity);
        }
        self.pointer = Some((x, y));
        // Keep the hidden pointer near the middle so it never hits the window's edge.
        let (cx, cy) = (self.window_size.0 / 2.0, self.window_size.1 / 2.0);
        if ((x - cx).abs() > self.window_size.0 * 0.2 || (y - cy).abs() > self.window_size.1 * 0.2)
            && crate::capture::warp_by((cx - x) as f64, (cy - y) as f64, self.ui_scale as f64)
        {
            self.pointer = Some((cx, cy));
        }
    }

    pub fn drag(&mut self, phase: rune::Phase, x: f32, y: f32, dx: f32, dy: f32) {
        let capture = self.capture_enabled && self.playing();
        match phase {
            rune::Phase::Down => {
                if capture && !self.captured {
                    // The first click into the window grabs the mouse.
                    self.captured = true;
                    self.pointer = Some((x, y));
                    return;
                }
                self.look = Look { down: true, dragging: false, last: (0.0, 0.0) };
                self.pointer = Some((x, y));
                self.mine_click();
            }
            rune::Phase::Move => {
                if self.mouse_look() {
                    self.pointer_at(x, y);
                    return;
                }
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
            self.select(self.selected + step);
            self.wheel_acc -= 24.0 * self.wheel_acc.signum();
        }
    }

    /// Throw the held item (Q); with Ctrl, the whole stack.
    pub fn drop_held(&mut self) {
        let Some(s) = self.inv[self.selected] else { return };
        let n = if self.ctrl { s.n } else { 1 };
        self.throw(Stack { n, ..s }, 6.0);
        self.inv[self.selected] = if s.n > n { Some(Stack { n: s.n - n, ..s }) } else { None };
    }

    /// A press: start breaking the block you're looking at.
    fn mine_click(&mut self) {
        if !self.playing() {
            return;
        }
        self.swing = 1.0;
        if self.mode == Mode::Creative {
            self.break_target();
            self.break_cooldown = 0.3;
        }
    }

    fn particle(&mut self, pos: Vec3, tile: u8, size: f32, glow: bool) {
        let n = self.particles.len() as i32;
        let r = |k: i32| unit(hash3(self.t.to_bits(), k, n, 7)) - 0.5;
        let vel = Vec3::new(r(1), r(2) + 0.7, r(3)) * 5.0;
        let life = 0.6 + r(4) * 0.4;
        self.particles.push(Particle { pos, vel, tile, life, size, gravity: 14.0, glow });
    }

    /// What breaking a block gives you, in survival.
    fn drops(&self, b: BlockId, p: IVec3) -> Vec<Stack> {
        let held = self.held_item().map_or(0, |s| s.id);
        let roll = unit(hash3(self.t.to_bits(), p.x, p.y, p.z));
        let one = |id: BlockId| vec![Stack::new(id, 1)];
        match b {
            block::TALL_GRASS => {
                if roll < 0.15 { one(block::SEEDS) } else { vec![] }
            }
            block::OAK_LEAVES | block::BIRCH_LEAVES | block::SPRUCE_LEAVES | block::CHERRY_LEAVES => {
                if roll < 0.06 {
                    one(block::SAPLING)
                } else if roll < 0.11 && b == block::OAK_LEAVES {
                    one(block::APPLE)
                } else {
                    vec![]
                }
            }
            block::WHEAT_3 => vec![Stack::new(block::WHEAT, 1), Stack::new(block::SEEDS, 1 + (roll * 2.0) as u8)],
            _ => {
                let d = block::def(b).drop;
                if d != block::AIR && block::can_harvest(held, b) { one(d) } else { vec![] }
            }
        }
    }

    fn break_target(&mut self) {
        let Some(hit) = self.target else { return };
        let b = hit.block;
        if block::def(b).hardness < 0.0 && self.mode == Mode::Survival {
            return;
        }
        let p = hit.pos;
        let tex = block::def(b).tex[1];
        for _ in 0..10 {
            self.particle(p.as_vec3() + Vec3::splat(0.5), tex, 0.12, false);
        }
        self.sound(Sound::Break(material_sound(b)), Some(p.as_vec3()));
        self.world.set(p.x, p.y, p.z, block::AIR);
        self.send(ToServer::SetBlock { pos: p.to_array(), block: block::AIR });
        if self.mode == Mode::Survival {
            let center = p.as_vec3() + Vec3::splat(0.5);
            for d in self.drops(b, p) {
                self.send(ToServer::Drop { stack: d, pos: center.to_array(), vel: [0.0, 2.0, 0.0] });
            }
            if block::def(b).hardness > 0.2 {
                self.wear_held();
            }
            self.hunger = (self.hunger - 0.03).max(0.0);
        }
    }

    /// Right click: talk, use a block, or use/place the held item.
    pub fn use_click(&mut self) {
        if !self.playing() || self.use_cooldown > 0.0 {
            return;
        }
        self.use_cooldown = 0.18;
        self.swing = 1.0;
        let held = self.held_item();
        if let Some(hit) = self.target
            && !self.shift
            && self.use_block(hit)
        {
            return;
        }
        let Some(s) = held else { return };
        if let Some(food) = block::food_value(s.id) {
            if self.hunger < 20.0 || self.mode == Mode::Creative {
                self.hunger = (self.hunger + food).min(20.0);
                self.sound(Sound::Eat, None);
                self.use_held();
            }
            return;
        }
        match s.id {
            block::BUCKET | block::WATER_BUCKET | block::LAVA_BUCKET => return self.use_bucket(s.id),
            id if block::armor(id).is_some() => {
                let (slot, _) = block::armor(id).unwrap();
                let old = self.armor[slot].replace(s);
                self.inv[self.selected] = old;
                self.sound(Sound::Place(0), None);
                return;
            }
            _ => {}
        }
        let Some(hit) = self.target else { return };
        match (s.id, hit.block) {
            (block::WOOD_HOE..=block::DIAMOND_HOE, block::GRASS | block::DIRT) if hit.normal.y == 1 => {
                self.set_block(hit.pos, block::FARMLAND);
                self.sound(Sound::Place(2), Some(hit.pos.as_vec3()));
                self.wear_held();
                return;
            }
            (block::SEEDS, block::FARMLAND) if hit.normal.y == 1 => {
                let p = hit.pos + IVec3::Y;
                if self.world.get(p.x, p.y, p.z) == block::AIR {
                    self.set_block(p, block::WHEAT_0);
                    self.use_held();
                }
                return;
            }
            _ => {}
        }
        if !block::is_block(s.id) {
            return;
        }
        self.place(s.id, hit);
    }

    fn set_block(&mut self, p: IVec3, b: BlockId) {
        self.world.set(p.x, p.y, p.z, b);
        self.send(ToServer::SetBlock { pos: p.to_array(), block: b });
    }

    /// Blocks you can use. True if the click did something.
    fn use_block(&mut self, hit: Hit) -> bool {
        let p = hit.pos;
        match hit.block {
            block::TNT => {
                self.send(ToServer::Ignite { pos: p.to_array() });
                true
            }
            block::CRAFTING_TABLE => {
                self.open(Panel::Crafting);
                true
            }
            block::CHEST | block::FURNACE | block::FURNACE_LIT => {
                self.send(ToServer::OpenContainer { pos: p.to_array() });
                self.sound(Sound::Door, Some(p.as_vec3()));
                self.open(Panel::Container);
                true
            }
            block::BED => {
                if (0.23..0.77).contains(&self.time_of_day) {
                    self.say("You can only sleep at night. Your spawn point is set here.");
                    self.spawn = p.as_vec3() + Vec3::new(0.5, 1.0, 0.5);
                    self.send(ToServer::Sleep(Some(p.to_array())));
                    self.send(ToServer::Sleep(None));
                } else {
                    self.send(ToServer::Sleep(Some(p.to_array())));
                    self.body = Body::new(p.as_vec3() + Vec3::new(0.5, 0.6, 0.5));
                    self.open(Panel::Sleeping);
                }
                true
            }
            b if block::door_parts(b).is_some() => {
                let (upper, open, axis) = block::door_parts(b).unwrap();
                self.set_block(p, block::door(upper, !open, axis));
                let other = if upper { p - IVec3::Y } else { p + IVec3::Y };
                if block::door_parts(self.world.get(other.x, other.y, other.z)).is_some() {
                    self.world.set(other.x, other.y, other.z, block::door(!upper, !open, axis));
                }
                self.sound(Sound::Door, Some(p.as_vec3()));
                true
            }
            _ => false,
        }
    }

    fn place(&mut self, id: BlockId, hit: Hit) {
        let replace = matches!(hit.block, block::TALL_GRASS | block::FLOWER_RED | block::FLOWER_YELLOW);
        let p = if replace { hit.pos } else { hit.pos + hit.normal };
        if !(0..HEIGHT - 1).contains(&p.y) {
            return;
        }
        let free = |s: &State, q: IVec3| {
            let cur = s.world.get(q.x, q.y, q.z);
            cur == block::AIR || block::fluid(cur).is_some() || matches!(cur, block::TALL_GRASS | block::FLOWER_RED | block::FLOWER_YELLOW)
        };
        if !free(self, p) {
            return;
        }
        let below = self.world.get(p.x, p.y - 1, p.z);
        let block_id = match id {
            block::DOOR => {
                let up = p + IVec3::Y;
                if !free(self, up) || !block::is_solid(below) || self.blocked_by_someone(p) || self.blocked_by_someone(up) {
                    return;
                }
                let axis = if self.yaw.sin().abs() > self.yaw.cos().abs() { 1 } else { 0 };
                self.set_block(p, block::door(false, false, axis));
                self.set_block(up, block::door(true, false, axis));
                self.sound(Sound::Place(1), Some(p.as_vec3()));
                self.use_held();
                return;
            }
            block::LADDER => {
                // The plate sits against the block you clicked.
                match (hit.normal.x, hit.normal.z) {
                    (0, 1) => block::ladder(0),
                    (-1, 0) => block::ladder(1),
                    (0, -1) => block::ladder(2),
                    (1, 0) => block::ladder(3),
                    _ => return,
                }
            }
            block::TORCH | block::RAIL => {
                if !block::is_solid(below) {
                    return;
                }
                id
            }
            block::SAPLING | block::FLOWER_RED | block::FLOWER_YELLOW | block::TALL_GRASS => {
                if !matches!(below, block::GRASS | block::DIRT | block::FARMLAND) {
                    return;
                }
                id
            }
            block::WHEAT_0..=block::WHEAT_3 => return,
            _ => id,
        };
        if !block::collision_boxes(block_id).is_empty() && self.blocked_by_someone(p) {
            return;
        }
        self.set_block(p, block_id);
        self.sound(Sound::Place(material_sound(block_id)), Some(p.as_vec3()));
        self.use_held();
    }

    fn blocked_by_someone(&self, p: IVec3) -> bool {
        self.body.overlaps_block(p) || self.others.values().any(|o| Body::new(o.pos).overlaps_block(p))
    }

    fn use_bucket(&mut self, id: BlockId) {
        let world = &self.world;
        let get = |x: i32, y: i32, z: i32| world.get(x, y, z);
        let Some(hit) = raycast_fluids(self.eye(), self.forward(), 5.0, &get) else { return };
        if id == block::BUCKET {
            let filled = match hit.block {
                block::WATER => block::WATER_BUCKET,
                block::LAVA => block::LAVA_BUCKET,
                _ => return,
            };
            self.set_block(hit.pos, block::AIR);
            self.sound(Sound::Splash, Some(hit.pos.as_vec3()));
            if self.mode == Mode::Survival {
                self.use_held();
                self.give_or_drop(Stack::new(filled, 1));
            }
            return;
        }
        let p = if block::fluid(hit.block).is_some() { hit.pos } else { hit.pos + hit.normal };
        let cur = self.world.get(p.x, p.y, p.z);
        if cur != block::AIR && block::fluid(cur).is_none() && block::def(cur).kind != Kind::Plant {
            return;
        }
        self.set_block(p, if id == block::WATER_BUCKET { block::WATER } else { block::LAVA });
        self.sound(Sound::Splash, Some(p.as_vec3()));
        if self.mode == Mode::Survival {
            self.inv[self.selected] = Some(Stack::new(block::BUCKET, 1));
        }
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
                    self.got_welcome = false;
                    self.leave();
                    self.status = why;
                    return;
                }
                NetEvent::Msg(m) => self.message(m),
            }
        }
    }

    fn message(&mut self, m: ToClient) {
        match m {
            ToClient::Welcome { id, seed, spawn, time, world, motd, save } => {
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
                match save {
                    Some(s) => {
                        for (i, st) in s.inv.iter().take(INV).enumerate() {
                            self.inv[i] = *st;
                        }
                        for (i, st) in s.armor.iter().take(4).enumerate() {
                            self.armor[i] = *st;
                        }
                        self.health = if s.health > 0.0 { s.health } else { 20.0 };
                        self.hunger = s.hunger;
                        self.mode = if s.creative { Mode::Creative } else { Mode::Survival };
                    }
                    None => {
                        self.health = 20.0;
                        self.hunger = 20.0;
                        self.armor = [None; 4];
                        self.inv = starter(self.mode);
                    }
                }
                self.hint_until = self.t + 14.0;
                self.say(motd);
                let how = if self.capture_enabled { "Click to grab the mouse, Esc to let go." } else { "Drag to look around." };
                self.say(format!("{how} E: inventory · M: map and cities · T: chat · F5: camera"));
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
                    self.others.insert(id, Other { name, skin, target: pose, pos, yaw: pose.yaw, pitch: pose.pitch, walk: 0.0, speed: 0.0, held: 0 });
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
            ToClient::PlayerHeld { id, item } => {
                if let Some(o) = self.others.get_mut(&id) {
                    o.held = item;
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
            ToClient::Items(list) => {
                let seen: HashSet<u32> = list.iter().map(|m| m.id).collect();
                self.items.retain(|id, _| seen.contains(id));
                for m in list {
                    let p = Vec3::from(m.pos);
                    let v = self.items.entry(m.id).or_insert(ItemView { item: m.item, target: p, pos: p });
                    v.target = p;
                    v.item = m.item;
                }
            }
            ToClient::Damage { amount, from } => {
                let k = 1.0 - (self.armor_points() * 0.04).min(0.8);
                self.hurt(amount * k);
                let push = (self.body.pos - Vec3::from(from)).with_y(0.0).normalize_or_zero();
                self.body.vel += push * 6.0 + Vec3::Y * 4.0;
            }
            ToClient::Explosion { pos } => {
                let p = Vec3::from(pos);
                self.explosions.push((p, self.t));
                let d = p.distance(self.body.pos);
                self.shake = self.shake.max((1.0 - d / 30.0).max(0.0));
                self.sound(Sound::Explosion, Some(p));
                for i in 0..30 {
                    let a = i as f32 * 0.7;
                    self.particles.push(Particle {
                        pos: p,
                        vel: Vec3::new(a.cos(), (i % 5) as f32 * 0.3 + 0.2, a.sin()) * 6.0,
                        tile: block::tile::SMOKE,
                        life: 1.2,
                        size: 0.5,
                        gravity: -1.0,
                        glow: false,
                    });
                }
            }
            ToClient::Pickup(stack) => {
                self.give_or_drop(stack);
                self.sound(Sound::Pickup, None);
            }
            ToClient::Container { pos, kind, slots, burn, progress } => {
                if self.panel == Panel::Container {
                    self.container = Some(ContainerView { pos, kind, slots, burn, progress });
                }
            }
            ToClient::Weather { rain, thunder } => {
                self.rain_target = rain;
                self.thunder = thunder;
            }
            ToClient::Lightning { pos } => {
                let p = Vec3::from(pos);
                self.lightning.push((p, self.t));
                let d = p.distance(self.body.pos);
                let heard = self.body.pos + (p - self.body.pos).normalize_or_zero() * (d / 8.0).min(28.0);
                self.sound(Sound::Thunder, Some(heard));
            }
            ToClient::SpawnPoint(p) => self.spawn = Vec3::from(p),
            ToClient::WakeUp => {
                if self.panel == Panel::Sleeping {
                    self.panel = Panel::None;
                }
            }
            ToClient::Kicked(why) => {
                self.got_welcome = false;
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
        self.sound(Sound::Hurt, None);
    }

    pub fn leave_bed(&mut self) {
        self.send(ToServer::Sleep(None));
        self.panel = Panel::None;
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
        self.time_of_day = (self.time_of_day + dt / runecraft_common::DAY_LENGTH).fract();
        self.rain += (self.rain_target - self.rain).clamp(-dt * 0.2, dt * 0.2);
        self.hurt_flash = (self.hurt_flash - dt * 2.0).max(0.0);
        self.shake = (self.shake - dt * 1.5).max(0.0);
        self.swing = (self.swing - dt * 4.0).max(0.0);
        self.break_cooldown = (self.break_cooldown - dt).max(0.0);
        self.use_cooldown = (self.use_cooldown - dt).max(0.0);
        self.explosions.retain(|e| t - e.1 < 1.0);
        self.lightning.retain(|e| t - e.1 < 0.35);
        if !self.playing() {
            self.captured = false;
        }

        let feet = self.body.pos.floor().as_ivec3();
        let ready = self.world.loaded(feet.x, feet.z);
        if ready && self.panel != Panel::Sleeping {
            self.step_player(dt);
        }
        self.update_target();
        if ready {
            self.step_mining(dt);
            self.step_survival(dt);
        }
        self.step_particles(dt);
        self.send_pose();
        self.animate_others(dt);
        let c = self.body.pos.floor().as_ivec3();
        self.world.pump((c.x.div_euclid(CHUNK), c.y.div_euclid(CHUNK), c.z.div_euclid(CHUNK)));
        self.view_proj = self.camera();
        if self.show_minimap && self.t - self.map_at > 0.5 && self.panel == Panel::None {
            self.map_at = self.t;
            crate::map::draw(self, 128, 1, true);
        }
        if self.t - self.last_save > 10.0 {
            self.last_save = self.t;
            self.save_player();
        }
        let held = self.held_item().map_or(0, |s| s.id);
        if held != self.last_held {
            self.last_held = held;
            self.send(ToServer::Held(held));
        }
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
        } else if self.sprint && (self.hunger > 6.0 || self.mode == Mode::Creative) {
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
                    .any(|(dx, dz)| !block::collision_boxes(world.get_solid((p.x + dx).floor() as i32, (p.y - 0.5).floor() as i32, (p.z + dz).floor() as i32)).is_empty())
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
        // Footsteps and splashes.
        if self.body.on_ground && !self.flying {
            self.step_dist += (self.body.pos - before.pos).with_y(0.0).length();
            if self.step_dist > 1.8 {
                self.step_dist = 0.0;
                let p = self.body.pos.floor().as_ivec3();
                let under = self.world.get(p.x, p.y - 1, p.z);
                self.sound(Sound::Step(material_sound(under)), None);
            }
        }
        if self.body.in_water && !before.in_water && before.vel.y < -4.0 {
            self.sound(Sound::Splash, None);
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
        let eye = self.eye();
        let dir = self.forward();
        self.target = if self.screen == Screen::Playing { raycast(eye, dir, reach, &get) } else { None };
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
        self.dig_sound -= dt;
        if self.dig_sound <= 0.0 {
            self.dig_sound = 0.25;
            self.sound(Sound::Dig(material_sound(hit.block)), Some(hit.pos.as_vec3()));
        }
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
        self.hunger = (self.hunger - dt * (0.01 + if self.sprint { 0.04 } else { 0.0 } + moving * 0.0015)).max(0.0);
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
        if self.touching_cactus() {
            self.regen += dt;
            if self.regen > 0.6 {
                self.regen = 0.0;
                self.hurt(1.0);
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
            self.die();
        }
    }

    fn touching_cactus(&self) -> bool {
        let b = self.body.pos;
        [(0.32, 0.0), (-0.32, 0.0), (0.0, 0.32), (0.0, -0.32)]
            .iter()
            .any(|(dx, dz)| [0.2, 1.0].iter().any(|dy| self.world.get((b.x + dx).floor() as i32, (b.y + dy).floor() as i32, (b.z + dz).floor() as i32) == block::CACTUS))
    }

    fn die(&mut self) {
        self.health = 0.0;
        self.close_panel();
        // Everything you carried scatters where you fell.
        let items: Vec<Stack> = self.inv.iter_mut().chain(self.armor.iter_mut()).filter_map(|s| s.take()).collect();
        let at = self.body.pos + Vec3::Y;
        for (i, s) in items.into_iter().enumerate() {
            let a = i as f32 * 2.4;
            self.send(ToServer::Drop { stack: s, pos: at.to_array(), vel: [a.cos() * 3.0, 3.0, a.sin() * 3.0] });
        }
        self.open(Panel::Dead);
        self.save_player();
    }

    fn step_particles(&mut self, dt: f32) {
        for p in &mut self.particles {
            p.vel.y -= p.gravity * dt;
            p.pos += p.vel * dt;
            p.life -= dt;
        }
        let world = &self.world;
        self.particles.retain(|p| p.life > 0.0 && !block::is_solid(world.get(p.pos.x.floor() as i32, p.pos.y.floor() as i32, p.pos.z.floor() as i32)));
        if self.particles.len() > 1500 {
            let n = self.particles.len() - 1500;
            self.particles.drain(0..n);
        }
        // Rain and snow around you, where the sky is open.
        if self.rain > 0.05 {
            let eye = self.body.pos;
            let biome = runecraft_common::worldgen::sample(self.seed, eye.x as i32, eye.z as i32).biome;
            let cold = matches!(biome, runecraft_common::worldgen::Biome::Snowy) || eye.y > 140.0;
            if !matches!(biome, runecraft_common::worldgen::Biome::Desert) {
                let n = (self.rain * if cold { 10.0 } else { 24.0 } * dt * 60.0) as i32;
                for i in 0..n {
                    let r = |k: i32| unit(hash3(self.t.to_bits(), i, k, 3)) - 0.5;
                    let pos = eye + Vec3::new(r(1) * 36.0, 10.0 + r(2) * 8.0, r(3) * 36.0);
                    let b = pos.floor().as_ivec3();
                    if !self.world.sky_exposed(b.x, b.y, b.z) {
                        continue;
                    }
                    let (vel, tile, size) = if cold {
                        (Vec3::new(r(4) * 0.6, -2.0, r(5) * 0.6), block::tile::SNOWFLAKE, 0.08)
                    } else {
                        (Vec3::new(0.0, -22.0, 0.0), block::tile::RAIN, 0.0)
                    };
                    self.particles.push(Particle { pos, vel, tile, life: 1.6, size, gravity: 0.0, glow: false });
                }
            }
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
        for it in self.items.values_mut() {
            if it.pos.distance(it.target) > 6.0 {
                it.pos = it.target;
            } else {
                it.pos += (it.target - it.pos) * k;
            }
        }
    }

    /// Nearest city, its distance in blocks, and its compass direction.
    pub fn nearest_city(&self) -> Option<(&'static str, f32, &'static str)> {
        let p = self.body.pos;
        CITIES
            .iter()
            .map(|c| (c, Vec3::new(c.x as f32 - p.x, 0.0, c.z as f32 - p.z)))
            .min_by(|a, b| a.1.length().total_cmp(&b.1.length()))
            .map(|(c, d)| (c.name, d.length(), compass(d)))
    }

    pub fn biome_here(&self) -> &'static str {
        let p = self.body.pos.floor().as_ivec3();
        runecraft_common::worldgen::sample(self.seed, p.x, p.z).biome.name()
    }
}

pub fn compass(d: Vec3) -> &'static str {
    let ang = d.x.atan2(-d.z).to_degrees().rem_euclid(360.0);
    ["N", "NE", "E", "SE", "S", "SW", "W", "NW"][(((ang + 22.5) / 45.0) as usize) % 8]
}

impl Default for State {
    fn default() -> Self {
        State::new()
    }
}
