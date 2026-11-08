//! RuneCraft: a multiplayer block-building game with real cities, built on the Rune UI
//! framework. Rune draws the menus and HUD; the 3D world is drawn by our own wgpu renderer
//! plugged in as Rune's compositor.

mod game;
mod mesh;
mod net;
mod render;
mod textures;
mod ui;
mod world;

use game::State;
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
    s.world_name = rune::TextEdit::new("snapshot-world");
    s.seed_text = rune::TextEdit::new("2026");
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

fn city(i: usize) -> (f32, f32, f32) {
    runecraft_common::city::CITIES[i].arrival()
}

fn main() {
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
            ("night", |s| play_and_wait(s, Some(city(1)), 3.6, 0.3, 0.9)),
            ("friends", friend_scene),
            ("inventory", |s| {
                play_and_wait(s, None, 0.6, -0.15, 0.3);
                s.panel = game::Panel::Inventory;
            }),
            ("survival", |s| {
                play_and_wait(s, None, 0.6, -0.15, 0.3);
                s.mode = game::Mode::Survival;
                s.inv = [None; game::INV];
                s.give(runecraft_common::block::OAK_LOG, 6);
                s.give(runecraft_common::block::COBBLE, 12);
                s.health = 15.0;
                s.panel = game::Panel::Inventory;
            }),
            ("travel", |s| {
                play_and_wait(s, None, 0.6, -0.15, 0.3);
                s.panel = game::Panel::Travel;
            }),
        ],
        ..AppSpec::new("RuneCraft", (1280.0, 800.0), State::new, ui::view)
    };
    App::new(spec).modifier_keys().compositor(render::Gfx::default()).run();
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
        let cfg = runecraft_server::Config { bind: "127.0.0.1:0".into(), world_dir: dir.clone(), mobs: false, seed: 99, ..Default::default() };
        let server = runecraft_server::start(cfg).unwrap();
        let mut a = State::new();
        let mut b = State::new();
        for (s, n) in [(&mut a, "alice"), (&mut b, "bob")] {
            s.name = rune::TextEdit::new(n);
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
        a.drag(rune::Phase::Down, 0.0, 0.0);
        a.drag(rune::Phase::Up, 0.0, 0.0);
        assert_eq!(a.world.get(target.x, target.y, target.z), block::AIR);
        assert!(pump(&mut [&mut a, &mut b], &mut t, |v| v[1].world.get(target.x, target.y, target.z) == block::AIR), "bob sees the hole");

        // And chats.
        a.chat_edit = rune::TextEdit::new("hello bob");
        a.submit_chat();
        assert!(pump(&mut [&mut a, &mut b], &mut t, |v| v[1].chat.iter().any(|l| l.text == "<alice> hello bob")), "bob reads the chat");

        drop(server);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn headless_game() -> rune::Runtime<State> {
        let mut s = State::new();
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
}
