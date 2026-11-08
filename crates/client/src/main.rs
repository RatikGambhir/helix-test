//! RuneCraft: a multiplayer block-building sandbox with real cities, built on the Rune UI
//! framework. Rune draws the menus and HUD; the 3D world is drawn by our own wgpu renderer
//! plugged in as Rune's compositor.

mod audio;
mod capture;
mod game;
mod map;
mod mesh;
mod net;
mod render;
mod textures;
mod ui;
mod world;

use game::State;
use runecraft_common::block;
use rune::{App, AppSpec, Tx, Waker};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

fn poll(s: &mut State, t: f64, _tx: &mut Tx) -> bool {
    s.update(t)
}

fn setup(s: &mut State, w: Waker) {
    s.waker = Some(w);
}

/// This machine's address on the local network, for friends joining a LAN game.
pub fn local_ip() -> String {
    static IP: OnceLock<String> = OnceLock::new();
    IP.get_or_init(|| {
        // Connecting a UDP socket sends nothing; it just picks the outgoing interface.
        std::net::UdpSocket::bind("0.0.0.0:0")
            .and_then(|s| s.connect("192.0.2.1:9").map(|_| s))
            .and_then(|s| s.local_addr())
            .map(|a| a.ip().to_string())
            .unwrap_or_else(|_| "127.0.0.1".into())
    })
    .clone()
}

/// Headless scenes: start a world and wait (in real time) until the terrain around the
/// player is loaded and meshed, so `--snap` shows the game.
fn play_and_wait(s: &mut State, travel: Option<(f32, f32, f32)>, yaw: f32, pitch: f32, time: f32) {
    s.view_distance = 8;
    s.sound_on = false;
    s.capture_enabled = false;
    s.world_name = rune::TextEdit::new("snapshot-world");
    s.seed_text = rune::TextEdit::new(std::env::var("RUNECRAFT_SEED").unwrap_or_else(|_| "2026".into()));
    s.host(false);
    let start = Instant::now();
    let mut t = 0.0;
    let mut travelled = travel.is_none();
    let mut settled = 0;
    while start.elapsed() < Duration::from_secs(90) {
        t += 0.05;
        s.update(t);
        if s.got_welcome && !travelled {
            let (x, y, z) = travel.unwrap();
            s.travel(glam::Vec3::new(x, y, z));
            travelled = true;
        }
        s.yaw = yaw;
        s.pitch = pitch;
        s.time_of_day = time;
        if std::env::var_os("RUNECRAFT_AT").is_some() {
            s.flying = true;
            s.mode = game::Mode::Creative;
        }
        let feet = s.body.pos.floor().as_ivec3();
        let ready = travelled && s.world.loaded(feet.x, feet.z) && s.world.columns.len() > 200 && s.world.pending() == 0;
        settled = if ready { settled + 1 } else { 0 };
        if settled > 10 {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    s.hint_until = 0.0;
    s.chat.clear();
    s.view_proj = s.camera();
    map::draw(s, 128, 1, true);
}

/// A scripted friend who connects to the hosted world and stands in front of you.
fn friend_scene(s: &mut State) {
    play_and_wait(s, None, 2.2, -0.1, 0.35);
    let Some(addr) = s.server.as_ref().map(|sv| sv.addr) else { return };
    let fwd = glam::Vec3::new(s.yaw.sin(), 0.0, -s.yaw.cos());
    let mut at = s.body.pos + fwd * 4.0 + glam::Vec3::new(fwd.z, 0.0, -fwd.x) * 1.0;
    let col = at.floor().as_ivec3();
    let mut y = 250;
    while y > 0 && !runecraft_common::block::is_opaque(s.world.get(col.x, y, col.z)) {
        y -= 1;
    }
    at.y = y as f32 + 1.0;
    let face = s.yaw + std::f32::consts::PI;
    std::thread::spawn(move || {
        use runecraft_common::net::{read_frame, write_frame};
        use runecraft_common::protocol::*;
        let Ok(mut out) = std::net::TcpStream::connect(addr) else { return };
        let mut inp = std::io::BufReader::new(out.try_clone().unwrap());
        std::thread::spawn(move || while read_frame::<ToClient, _>(&mut inp).is_ok() {});
        let _ = write_frame(&mut out, &ToServer::Hello { name: "Yuki".into(), version: runecraft_common::PROTOCOL_VERSION, view_distance: 2, skin: 13 });
        for _ in 0..1200 {
            let _ = write_frame(&mut out, &ToServer::Move(Pose { pos: at.to_array(), yaw: face, pitch: 0.1 }));
            std::thread::sleep(Duration::from_millis(100));
        }
    });
    let start = Instant::now();
    let mut t = 100.0;
    while start.elapsed() < Duration::from_secs(8) {
        t += 0.05;
        s.update(t);
        std::thread::sleep(Duration::from_millis(50));
    }
    s.chat.retain(|l| l.text.contains("joined"));
    s.hint_until = 0.0;
}

/// Looks at a point.
pub fn aim(s: &mut State, at: glam::Vec3) {
    let d = at - s.eye();
    s.yaw = d.x.atan2(-d.z).rem_euclid(std::f32::consts::TAU);
    s.pitch = (d.y / d.length()).asin();
}

/// Places the held block against `face` of block `on`, the way a player does: look at that
/// face and right-click. True if the block went in.
pub fn place_against(s: &mut State, on: glam::IVec3, face: glam::IVec3) -> bool {
    let at = on.as_vec3() + glam::Vec3::splat(0.5) + face.as_vec3() * 0.5;
    aim(s, at);
    let t = s.t + 0.02;
    s.update(t);
    match s.target {
        Some(h) if h.pos == on && h.normal == face => {
            s.use_cooldown = 0.0;
            let before = s.world.get(on.x + face.x, on.y + face.y, on.z + face.z);
            s.use_click();
            s.world.get(on.x + face.x, on.y + face.y, on.z + face.z) != before
        }
        other => {
            if std::env::var_os("RUNECRAFT_DEBUG").is_some() {
                eprintln!("aim at {on:?}{face:?} from {:?} hit {other:?}", s.eye());
            }
            false
        }
    }
}

fn ground_at(s: &State, x: i32, z: i32) -> (i32, u8) {
    let mut y = 200;
    while y > 0 && !matches!(block::def(s.world.get(x, y, z)).kind, block::Kind::Solid) {
        y -= 1;
    }
    (y, s.world.get(x, y, z))
}

/// A gently sloped patch of grass `n`×`n` near the player with open sky: (corner x, top ground y, corner z).
fn flat_site(s: &State, n: i32) -> Option<(i32, i32, i32)> {
    let c = s.body.pos.floor().as_ivec3();
    for r in 4i32..70 {
        for dz in -r..=r {
            for dx in -r..=r {
                if dx.abs() != r && dz.abs() != r {
                    continue;
                }
                let (x0, z0) = (c.x + dx, c.z + dz);
                let cells: Vec<(i32, u8)> = (0..n).flat_map(|i| (0..n).map(move |j| (i, j))).map(|(i, j)| ground_at(s, x0 + i, z0 + j)).collect();
                let lo = cells.iter().map(|c| c.0).min()?;
                let hi = cells.iter().map(|c| c.0).max()?;
                let soil = cells.iter().all(|c| matches!(c.1, block::GRASS | block::DIRT | block::SAND));
                if hi - lo > 2 || !soil {
                    continue;
                }
                let clear = (0..n).all(|i| (0..n).all(|j| ((hi + 1)..(hi + 9)).all(|y| !block::is_solid(s.world.get(x0 + i, y, z0 + j)))));
                let view = (-12..0).all(|j| ((hi + 1)..(hi + 6)).all(|y| !block::is_solid(s.world.get(x0 + 4, y, z0 + j))));
                if clear && view {
                    return Some((x0, hi, z0));
                }
            }
        }
    }
    None
}

/// Builds a little house with the same aim-and-right-click a player uses, then steps back to look.
fn house_scene(s: &mut State) {
    play_and_wait(s, None, 0.6, -0.15, 0.3);
    s.mode = game::Mode::Creative;
    let Some((x0, h, z0)) = flat_site(s, 7) else {
        eprintln!("house: no flat spot near spawn");
        return;
    };
    let put = |s: &mut State, slot: usize, id: u8| s.inv[slot] = Some(block::Stack::new(id, block::max_stack(id)));
    for (i, id) in [block::PLANKS, block::GLASS, block::OAK_LOG, block::DOOR, block::TORCH, block::SLATE, block::CRAFTING_TABLE, block::CHEST, block::BED].into_iter().enumerate() {
        put(s, i, id);
    }
    let (cx, cz) = (x0 + 3, z0 + 3);
    s.flying = true;
    // Hover over the middle, within reach (7 blocks) of every corner.
    s.body = runecraft_common::physics::Body::new(glam::Vec3::new(cx as f32 + 0.5, (h + 4) as f32, cz as f32 + 0.5));
    let up = glam::IVec3::Y;
    let mut placed = 0;
    // Level the ground: fill low spots with cobblestone up to the highest corner.
    s.inv[9] = Some(block::Stack::new(block::COBBLE, 64));
    s.inv.swap(9, 0);
    s.select(0);
    let mut cells: Vec<(i32, i32)> = (0..7).flat_map(|i| (0..7).map(move |j| (i, j))).collect();
    cells.sort_by_key(|&(i, j)| -((i - 3).abs() + (j - 3).abs()));
    for (i, j) in cells {
        {
            loop {
                let (g, _) = ground_at(s, x0 + i, z0 + j);
                if g >= h || !place_against(s, glam::IVec3::new(x0 + i, g, z0 + j), up) {
                    break;
                }
                placed += 1;
            }
        }
    }
    s.inv.swap(9, 0);
    // Furniture first, while the inside is still open to the sky.
    for (slot, x, z) in [(4, cx, z0 + 5), (6, x0 + 1, z0 + 5), (7, x0 + 5, z0 + 5), (8, x0 + 5, z0 + 2)] {
        s.select(slot);
        placed += place_against(s, glam::IVec3::new(x, h, z), up) as i32;
    }
    // Walls: log corners, plank walls, glass windows, a door at the front.
    for y in (h + 1)..=(h + 4) {
        for i in 0..7 {
            for j in 0..7 {
                let edge = i == 0 || i == 6 || j == 0 || j == 6;
                if !edge {
                    continue;
                }
                let corner = (i == 0 || i == 6) && (j == 0 || j == 6);
                let door = j == 0 && i == 3 && y <= h + 2;
                if door {
                    if y == h + 1 {
                        s.select(3);
                        placed += place_against(s, glam::IVec3::new(x0 + i, h, z0 + j), up) as i32;
                    }
                    continue;
                }
                let window = !corner && y == h + 2 && (i == 2 || i == 4 || j == 2 || j == 4);
                s.select(if corner { 2 } else if window { 1 } else { 0 });
                let ok = if j == 0 && i == 3 {
                    // Above the door: against the wall block beside it.
                    place_against(s, glam::IVec3::new(x0 + 2, y, z0), glam::IVec3::X)
                } else {
                    place_against(s, glam::IVec3::new(x0 + i, y - 1, z0 + j), up)
                };
                placed += ok as i32;
            }
        }
    }
    // A slate roof, from the edges inward, placed from above it.
    s.body = runecraft_common::physics::Body::new(glam::Vec3::new(cx as f32 + 0.5, (h + 6) as f32 + 0.5, cz as f32 + 0.5));
    s.select(5);
    let ry = h + 5;
    let mut roof = std::collections::HashSet::new();
    for i in -1..8 {
        for j in -1..8 {
            let on_wall = (0..7).contains(&i) && (0..7).contains(&j) && (i == 0 || i == 6 || j == 0 || j == 6);
            if on_wall && place_against(s, glam::IVec3::new(x0 + i, ry - 1, z0 + j), up) {
                roof.insert((i, j));
                placed += 1;
            }
        }
    }
    for _ in 0..8 {
        for i in -1..8 {
            for j in -1..8 {
                if roof.contains(&(i, j)) {
                    continue;
                }
                // Lean on a neighbour further from the middle, whose face points our way.
                let towards = glam::IVec3::new((3 - i).signum(), 0, (3 - j).signum());
                for d in [glam::IVec3::new(-towards.x, 0, 0), glam::IVec3::new(0, 0, -towards.z)] {
                    if d == glam::IVec3::ZERO || !roof.contains(&(i + d.x, j + d.z)) {
                        continue;
                    }
                    if place_against(s, glam::IVec3::new(x0 + i + d.x, ry, z0 + j + d.z), -d) {
                        roof.insert((i, j));
                        placed += 1;
                        break;
                    }
                }
            }
        }
    }
    s.say(format!("Built a house with {placed} blocks"));
    eprintln!("house: placed {placed} blocks at {x0},{h},{z0}");
    // Step back to admire it from the front, at dusk.
    s.flying = true;
    s.body = runecraft_common::physics::Body::new(glam::Vec3::new(cx as f32 + 4.5, (h + 3) as f32, z0 as f32 - 9.0));
    aim(s, glam::Vec3::new(cx as f32 + 0.5, (h + 3) as f32, cz as f32 + 0.5));
    s.time_of_day = 0.74;
    let start = Instant::now();
    let mut t = s.t;
    while start.elapsed() < Duration::from_secs(4) {
        t += 0.05;
        s.update(t);
        s.time_of_day = 0.74;
        std::thread::sleep(Duration::from_millis(50));
    }
    s.view_proj = s.camera();
}

fn city(i: usize) -> (f32, f32, f32) {
    runecraft_common::city::CITIES[i].arrival()
}

/// `runecraft --export-icon icon.png`: writes the app icon (a grass block) at 1024×1024.
fn export_icon(path: &str) {
    let atlas = textures::build_atlas();
    let small = atlas.icon(block::GRASS);
    let (n, k) = (1024usize, 32usize);
    let mut big = vec![0u8; n * n * 4];
    for y in 0..n {
        for x in 0..n {
            let (sx, sy) = (x / k, y / k);
            let o = (y * n + x) * 4;
            big[o..o + 4].copy_from_slice(&small[(sy * 32 + sx) * 4..(sy * 32 + sx) * 4 + 4]);
        }
    }
    rune::render::write_png(std::path::Path::new(path), n as u32, n as u32, &big).expect("write icon");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--export-icon") {
        export_icon(args.get(i + 1).map_or("icon.png", |s| s.as_str()));
        return;
    }
    rune::set_app_id("runecraft");
    let spec = AppSpec {
        min_size: Some((820.0, 560.0)),
        setup: Some(setup),
        poll: Some(poll),
        on_key: Some(ui::on_key),
        scenes: &[
            ("play", |s| play_and_wait(s, None, 0.6, -0.15, 0.3)),
            ("newyork", |s| play_and_wait(s, Some(city(0)), 0.0, 0.25, 0.32)),
            ("tokyo", |s| play_and_wait(s, Some(city(1)), 3.6, 0.3, 0.35)),
            ("paris", |s| play_and_wait(s, Some(city(2)), 3.14, 0.35, 0.3)),
            ("london", |s| play_and_wait(s, Some(city(3)), 3.0, 0.2, 0.3)),
            ("chicago", |s| play_and_wait(s, Some(city(4)), 0.3, 0.3, 0.3)),
            ("sydney", |s| play_and_wait(s, Some(city(5)), 1.07, 0.05, 0.3)),
            ("dubai", |s| play_and_wait(s, Some(city(6)), 2.36, 0.5, 0.3)),
            ("rome", |s| play_and_wait(s, Some(city(7)), 0.0, 0.15, 0.3)),
            ("night", |s| play_and_wait(s, Some(city(1)), 3.6, 0.3, 0.9)),
            ("friends", friend_scene),
            ("house", house_scene),
            ("at", |s| {
                // RUNECRAFT_AT="x,y,z,yaw,pitch[,time]": a camera anywhere, for screenshots.
                let v: Vec<f32> = std::env::var("RUNECRAFT_AT").unwrap_or_default().split(',').filter_map(|p| p.trim().parse().ok()).collect();
                if v.len() >= 5 {
                    play_and_wait(s, Some((v[0], v[1], v[2])), v[3], v[4], v.get(5).copied().unwrap_or(0.3));
                }
            }),
            ("inventory", |s| {
                play_and_wait(s, None, 0.6, -0.15, 0.3);
                s.mode = game::Mode::Survival;
                s.inv = [None; game::INV];
                for (id, n) in [(block::OAK_LOG, 6), (block::COBBLE, 24), (block::STICK, 8), (block::COAL, 5), (block::IRON_INGOT, 3), (block::BREAD, 2), (block::IRON_PICK, 1)] {
                    s.give(block::Stack::new(id, n));
                }
                s.inv[8] = Some(block::Stack { id: block::IRON_PICK, n: 1, dmg: 120 });
                s.armor[1] = Some(block::Stack::new(block::IRON_CHESTPLATE, 1));
                s.grid[0] = Some(block::Stack::new(block::PLANKS, 2));
                s.grid[3] = Some(block::Stack::new(block::PLANKS, 2));
                s.cursor = Some(block::Stack::new(block::TORCH, 4));
                s.pointer = Some((700.0, 260.0));
                s.health = 15.0;
                s.panel = game::Panel::Inventory;
            }),
            ("creative", |s| {
                play_and_wait(s, None, 0.6, -0.15, 0.3);
                s.mode = game::Mode::Creative;
                s.panel = game::Panel::Inventory;
            }),
            ("map", |s| {
                play_and_wait(s, None, 0.6, -0.15, 0.3);
                map::draw(s, 256, 2, false);
                s.panel = game::Panel::Map;
            }),
            ("rain", |s| {
                play_and_wait(s, None, 0.6, -0.05, 0.3);
                s.rain = 1.0;
                s.rain_target = 1.0;
                for _ in 0..60 {
                    let t = s.t + 0.016;
                    s.update(t);
                }
                s.hint_until = 0.0;
            }),
        ],
        ..AppSpec::new("RuneCraft", (1280.0, 800.0), State::new, ui::view)
    };
    // The 3D renderer is plain wgpu: on macOS use wgpu's Metal backend rather than Rune's own.
    App::new(spec).gpu_backend(rune::GpuBackend::Wgpu).modifier_keys().compositor(render::Gfx::default()).run();
}

#[cfg(test)]
mod tests {
    use super::*;
    use runecraft_common::block;


    fn pump(states: &mut [&mut State], t: &mut f64, until: impl Fn(&[&mut State]) -> bool) -> bool {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(60) {
            *t += 0.05;
            for s in states.iter_mut() {
                s.update(*t);
            }
            if until(states) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }

    #[test]
    fn two_players_walk_build_and_chat_together() {
        let dir = std::env::temp_dir().join(format!("runecraft-mp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let cfg = runecraft_server::Config { bind: "127.0.0.1:0".into(), world_dir: dir.clone(), seed: 99, ..Default::default() };
        let server = runecraft_server::start(cfg).unwrap();
        let mut a = State::new();
        let mut b = State::new();
        for (s, n) in [(&mut a, "alice"), (&mut b, "bob")] {
            s.name = rune::TextEdit::new(n);
            s.sound_on = false;
            s.capture_enabled = false;
            s.mode = game::Mode::Creative;
            s.addr = rune::TextEdit::new(server.addr.to_string());
            s.view_distance = 3;
            s.join();
        }
        let mut t = 0.0;
        let ready = |s: &State| {
            let f = s.body.pos.floor().as_ivec3();
            s.got_welcome && s.world.loaded(f.x, f.z) && s.body.on_ground && s.others.len() == 1
        };
        let ok = pump(&mut [&mut a, &mut b], &mut t, |v| v.iter().all(|s| ready(s)));
        assert!(ok, "both joined, landed and see each other");

        // Walk forward for a second: alice moves, and bob sees her move.
        a.flying = true;
        a.key_down(" ");
        let ground = a.body.pos;
        pump(&mut [&mut a, &mut b], &mut t, |v| v[0].body.pos.y > ground.y + 6.0);
        a.key_up(" ");
        let start = a.body.pos;
        a.key_down("w");
        pump(&mut [&mut a, &mut b], &mut t, |v| v[0].body.pos.distance(start) > 2.0);
        a.key_up("w");
        let moved = a.body.pos;
        assert!(moved.distance(start) > 2.0, "alice walked");
        assert!(pump(&mut [&mut a, &mut b], &mut t, |v| v[1].others.values().any(|o| o.pos.distance(moved) < 0.5)), "bob sees alice where she is");

        // Alice lands, looks down and breaks the block under her feet; bob's world updates.
        a.flying = false;
        pump(&mut [&mut a, &mut b], &mut t, |v| v[0].body.on_ground);
        a.pitch = -1.55;
        a.update(t);
        let target = a.target.expect("a block under alice").pos;
        a.drag(rune::Phase::Down, 0.0, 0.0, 0.0, 0.0);
        a.drag(rune::Phase::Up, 0.0, 0.0, 0.0, 0.0);
        assert_eq!(a.world.get(target.x, target.y, target.z), block::AIR);
        assert!(pump(&mut [&mut a, &mut b], &mut t, |v| v[1].world.get(target.x, target.y, target.z) == block::AIR), "bob sees the hole");

        // And chats.
        a.chat_edit = rune::TextEdit::new("hello bob");
        a.submit_chat();
        assert!(pump(&mut [&mut a, &mut b], &mut t, |v| v[1].chat.iter().any(|l| l.text == "<alice> hello bob")), "bob reads the chat");

        drop(server);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Rune's headless runtimes share some process-wide input state: tests that drive input take turns.
    static INPUT: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn headless_game() -> rune::Runtime<State> {
        let mut s = State::new();
        s.capture_enabled = false;
        s.sound_on = false;
        s.screen = game::Screen::Playing;
        let mut rt = rune::Runtime::new(s, ui::view, (1280.0, 800.0), rune::Env { system_dark: true, scale: 1.0 });
        rt.on_key = Some(ui::on_key);
        rt.frame(0.0);
        rt
    }

    fn text_key(k: &str) -> rune::KeyEvent {
        rune::KeyEvent { key: rune::Key::Text(k.into()), mods: Default::default() }
    }

    #[test]
    fn clicks_and_keys_reach_the_game_through_the_hud() {
        let _turn = INPUT.lock().unwrap_or_else(|e| e.into_inner());
        let mut rt = headless_game();
        // Press in the middle, right under the crosshair: the game hears it.
        rt.pointer_down(640.0, 400.0);
        rt.frame(1.0 / 60.0);
        assert!(rt.state.look.down, "a press starts mining");
        rt.pointer_move(700.0, 380.0);
        rt.frame(1.0 / 60.0);
        rt.pointer_up(700.0, 380.0);
        rt.frame(1.0 / 60.0);
        assert!(!rt.state.look.down);
        assert!(rt.state.yaw > 0.1, "dragging right turns right: {}", rt.state.yaw);
        rt.secondary_down(640.0, 400.0);
        rt.secondary_up(640.0, 400.0);
        rt.frame(1.0 / 60.0);
        assert!(rt.state.swing > 0.0, "right click uses");

        // Held movement keys, and the inventory toggle.
        rt.key(text_key("w"));
        rt.frame(1.0 / 60.0);
        assert!(rt.state.held.contains("w"));
        rt.key_up(text_key("w"));
        rt.frame(1.0 / 60.0);
        assert!(!rt.state.held.contains("w"));
        rt.key(text_key("e"));
        rt.frame(1.0 / 60.0);
        assert_eq!(rt.state.panel, game::Panel::Inventory);
        rt.key(rune::KeyEvent { key: rune::Key::Escape, mods: Default::default() });
        rt.frame(1.0 / 60.0);
        assert_eq!(rt.state.panel, game::Panel::None);
        rt.key(text_key("3"));
        rt.frame(1.0 / 60.0);
        assert_eq!(rt.state.selected, 2);
    }

    #[test]
    fn chat_opens_focused_and_sends() {
        let _turn = INPUT.lock().unwrap_or_else(|e| e.into_inner());
        let mut rt = headless_game();
        rt.key(text_key("t"));
        rt.frame(1.0 / 60.0);
        rt.frame(1.0 / 60.0);
        assert_eq!(rt.state.panel, game::Panel::Chat);
        for c in ["/", "g", "m", " ", "s"] {
            rt.key(text_key(c));
            rt.frame(1.0 / 60.0);
        }
        assert_eq!(rt.state.chat_edit.text, "/gm s", "typing goes into the chat box, not the game");
        assert!(rt.state.held.is_empty());
        rt.key(rune::KeyEvent { key: rune::Key::Enter, mods: Default::default() });
        rt.frame(1.0 / 60.0);
        assert_eq!(rt.state.panel, game::Panel::None);
        assert_eq!(rt.state.mode, game::Mode::Survival);
    }

    /// A survival session driven through Rune's real input: punch a tree, pick up the logs, craft
    /// planks and a crafting table in the inventory screen, place the table and open it.
    #[test]
    fn survival_punch_a_tree_craft_and_place_a_table() {
        let _turn = INPUT.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("runecraft-play-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let cfg = runecraft_server::Config { bind: "127.0.0.1:0".into(), world_dir: dir.clone(), seed: 7, ..Default::default() };
        let server = runecraft_server::start(cfg).unwrap();
        let mut s = State::new();
        s.capture_enabled = false;
        s.sound_on = false;
        s.view_distance = 4;
        s.mode = game::Mode::Survival;
        s.name = rune::TextEdit::new("tester");
        s.addr = rune::TextEdit::new(server.addr.to_string());
        s.join();
        let mut rt = rune::Runtime::new(s, ui::view, (1280.0, 800.0), rune::Env { system_dark: true, scale: 1.0 });
        rt.on_key = Some(ui::on_key);
        let mut t = 0.0f64;
        let mut frame = |rt: &mut rune::Runtime<State>, t: &mut f64| {
            *t += 0.05;
            rt.state.update(*t);
            // State changed outside a handler: rebuild the view, as the app's poll does.
            rt.invalidate();
            rt.frame(0.05);
            std::thread::sleep(Duration::from_millis(15));
        };
        let wait = |rt: &mut rune::Runtime<State>, t: &mut f64, f: &mut dyn FnMut(&mut rune::Runtime<State>, &mut f64), until: &dyn Fn(&State) -> bool| {
            let start = Instant::now();
            while start.elapsed() < Duration::from_secs(40) {
                f(rt, t);
                if until(&rt.state) {
                    return true;
                }
            }
            false
        };
        let ready = |s: &State| {
            let f = s.body.pos.floor().as_ivec3();
            s.got_welcome && s.world.loaded(f.x, f.z) && s.body.on_ground && s.world.columns.len() > 40 && s.world.pending() == 0
        };
        assert!(wait(&mut rt, &mut t, &mut frame, &ready), "joined and landed");
        assert_eq!(rt.state.mode, game::Mode::Survival);

        // Find the bottom log of a tree with room to stand beside it.
        let me = rt.state.body.pos.floor().as_ivec3();
        let mut spot = None;
        'search: for r in 1..56 {
            for dz in -r..=r {
                for dx in -r..=r {
                    let (x, z) = (me.x + dx, me.z + dz);
                    for y in 60..110 {
                        let w = &rt.state.world;
                        let log = matches!(w.get(x, y, z), block::OAK_LOG | block::BIRCH_LOG | block::SPRUCE_LOG);
                        if log && matches!(w.get(x, y - 1, z), block::DIRT | block::GRASS) {
                            let (sx, sz) = (x + 1, z);
                            if block::is_solid(w.get(sx, y - 1, sz)) && w.get(sx, y, sz) == block::AIR && w.get(sx, y + 1, sz) == block::AIR {
                                spot = Some((glam::IVec3::new(x, y, z), glam::Vec3::new(sx as f32 + 0.5, y as f32, sz as f32 + 0.5)));
                                break 'search;
                            }
                        }
                    }
                }
            }
        }
        let (log, stand) = spot.expect("a tree near spawn");
        rt.state.travel(stand);
        assert!(wait(&mut rt, &mut t, &mut frame, &|s: &State| s.body.pos.distance(stand) < 0.6 && s.body.on_ground), "walked up to the tree");
        let log_block = rt.state.world.get(log.x, log.y, log.z);

        // Hold the mouse on the log until it breaks (logs take a few seconds by hand).
        aim(&mut rt.state, log.as_vec3() + glam::Vec3::new(1.0, 0.25, 0.5));
        frame(&mut rt, &mut t);
        assert_eq!(rt.state.target.map(|h| h.pos), Some(log), "looking at the bottom log");
        rt.pointer_down(640.0, 400.0);
        frame(&mut rt, &mut t);
        assert!(rt.state.look.down, "the press reached the game");
        let broke = wait(&mut rt, &mut t, &mut frame, &|s: &State| s.world.get(log.x, log.y, log.z) == block::AIR);
        rt.pointer_up(640.0, 400.0);
        frame(&mut rt, &mut t);
        assert!(broke, "punching the log broke it");
        let has = |s: &State, id: u8| s.inv.iter().flatten().any(|st| st.id == id);
        assert!(wait(&mut rt, &mut t, &mut frame, &|s: &State| has(s, log_block)), "the log dropped and was picked up");

        // E opens the inventory; put the log in the 2×2 grid and take planks out.
        rt.key(rune::KeyEvent { key: rune::Key::Text("e".into()), mods: Default::default() });
        frame(&mut rt, &mut t);
        frame(&mut rt, &mut t);
        assert_eq!(rt.state.panel, game::Panel::Inventory);
        let slot = rt.state.inv.iter().position(|st| st.is_some_and(|st| st.id == log_block)).unwrap();
        let key = if slot < 9 { format!("hot0-{slot}") } else { format!("bag{}-{}", (slot - 9) / 9, (slot - 9) % 9) };
        let click = |rt: &mut rune::Runtime<State>, key: &str| {
            let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no element {key}"));
            rt.pointer_down(x, y);
            rt.pointer_up(x, y);
            rt.frame(0.016);
        };
        click(&mut rt, &key);
        assert!(rt.state.cursor.is_some_and(|c| c.id == log_block), "picked the log up");
        click(&mut rt, "grid0-0");
        assert!(rt.state.cursor.is_none());
        assert_eq!(rt.state.output().map(|o| (o.id, o.n)), Some((block::PLANKS, 4)), "the grid shows planks");
        click(&mut rt, "out");
        assert_eq!(rt.state.cursor.map(|c| (c.id, c.n)), Some((block::PLANKS, 4)));
        click(&mut rt, "grid1-1");
        // Planks in two cells... take them back and lay a 2×2 for a crafting table instead.
        click(&mut rt, "grid1-1");
        let planks = rt.state.cursor.unwrap();
        assert_eq!(planks.n, 4);
        for cell in ["grid0-0", "grid0-1", "grid1-0", "grid1-1"] {
            if rt.state.slot(game::Slot::Grid(match cell { "grid0-0" => 0, "grid0-1" => 1, "grid1-0" => 2, _ => 3 })).is_some() {
                continue;
            }
            let (x, y) = rt.find(cell).unwrap();
            rt.secondary_down(x, y);
            rt.secondary_up(x, y);
            rt.frame(0.016);
        }
        assert!(rt.state.cursor.is_none(), "one plank in each of the four cells");
        assert_eq!(rt.state.output().map(|o| o.id), Some(block::CRAFTING_TABLE), "four planks make a crafting table");
        click(&mut rt, "out");
        let hot = (0..9).find(|&i| rt.state.inv[i].is_none()).unwrap();
        click(&mut rt, &format!("hot0-{hot}"));
        assert!(has(&rt.state, block::CRAFTING_TABLE), "crafted a crafting table");
        rt.key(rune::KeyEvent { key: rune::Key::Escape, mods: Default::default() });
        frame(&mut rt, &mut t);
        assert_eq!(rt.state.panel, game::Panel::None, "Esc closes the inventory");

        // Pick the table on the hotbar with a number key, and place it on the ground.
        let slot = rt.state.inv.iter().position(|st| st.is_some_and(|st| st.id == block::CRAFTING_TABLE)).unwrap();
        if slot >= 9 {
            rt.state.inv.swap(slot, 4);
        }
        let slot = slot.min(4);
        rt.key(rune::KeyEvent { key: rune::Key::Text(format!("{}", slot + 1)), mods: Default::default() });
        frame(&mut rt, &mut t);
        assert_eq!(rt.state.selected, slot);
        let ground = log - glam::IVec3::Y;
        aim(&mut rt.state, ground.as_vec3() + glam::Vec3::new(0.5, 1.0, 0.5));
        frame(&mut rt, &mut t);
        assert_eq!(rt.state.target.map(|h| (h.pos, h.normal)), Some((ground, glam::IVec3::Y)), "looking at the ground where the tree stood");
        rt.secondary_down(640.0, 400.0);
        rt.secondary_up(640.0, 400.0);
        frame(&mut rt, &mut t);
        assert_eq!(rt.state.world.get(log.x, log.y, log.z), block::CRAFTING_TABLE, "the table is placed");
        assert!(!has(&rt.state, block::CRAFTING_TABLE), "and left the inventory");

        // Right-click it to open the 3×3 crafting grid.
        // (The rest of the trunk still floats above it, so click the table's side.)
        aim(&mut rt.state, log.as_vec3() + glam::Vec3::new(1.0, 0.4, 0.5));
        for _ in 0..6 {
            frame(&mut rt, &mut t);
        }
        rt.secondary_down(640.0, 400.0);
        rt.secondary_up(640.0, 400.0);
        frame(&mut rt, &mut t);
        assert_eq!(rt.state.panel, game::Panel::Crafting, "the crafting table opens");
        assert_eq!(rt.state.grid_size(), 3);

        drop(rt);
        drop(server);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
