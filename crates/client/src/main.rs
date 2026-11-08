//! Cubeterra: a multiplayer block-building game with real cities, built on the Rune UI
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

fn city(i: usize) -> (f32, f32, f32) {
    cubeterra_common::city::CITIES[i].arrival()
}

fn main() {
    rune::set_app_id("cubeterra");
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
            ("london", |s| play_and_wait(s, Some(city(3)), 1.9, 0.12, 0.3)),
            ("night", |s| play_and_wait(s, Some(city(1)), 3.6, 0.3, 0.9)),
            ("inventory", |s| {
                play_and_wait(s, None, 0.6, -0.15, 0.3);
                s.panel = game::Panel::Inventory;
            }),
            ("survival", |s| {
                play_and_wait(s, None, 0.6, -0.15, 0.3);
                s.mode = game::Mode::Survival;
                s.inv = [None; game::INV];
                s.give(cubeterra_common::block::OAK_LOG, 6);
                s.give(cubeterra_common::block::COBBLE, 12);
                s.health = 15.0;
                s.panel = game::Panel::Inventory;
            }),
            ("travel", |s| {
                play_and_wait(s, None, 0.6, -0.15, 0.3);
                s.panel = game::Panel::Travel;
            }),
        ],
        ..AppSpec::new("Cubeterra", (1280.0, 800.0), State::new, ui::view)
    };
    App::new(spec).modifier_keys().compositor(render::Gfx::default()).run();
}
