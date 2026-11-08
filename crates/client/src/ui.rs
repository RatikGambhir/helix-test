//! Everything drawn with Rune: the title screen, the in-game HUD and the menus.

use crate::game::{Mode, Panel, Screen, State};
use cubeterra_common::block::{self, BlockId};
use cubeterra_common::city::CITIES;
use cubeterra_common::craft::{RECIPES, Station};
use glam::Vec3;
use rune::tok::TextStyle;
use rune::{Color, Cx, El, Key, KeyEvent, Phase, ScrollSpec, Tx, WheelEvent, col, el, row, text, text_input};

fn srgb(v: f32) -> f32 {
    if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
}

pub fn rgba(hex: u32, a: f32) -> Color {
    let ch = |s: u32| srgb(((hex >> s) & 255) as f32 / 255.0);
    Color { r: ch(16), g: ch(8), b: ch(0), a }
}

fn white() -> Color {
    Color::WHITE
}

const BODY: TextStyle = TextStyle::new(14.0, 500.0);
const SMALL: TextStyle = TextStyle::new(12.0, 500.0);
const LABEL: TextStyle = TextStyle::new(13.0, 600.0);
const HEAD: TextStyle = TextStyle::new(22.0, 700.0);

fn label(s: impl Into<String>, style: TextStyle, c: Color) -> El {
    text(s).font(style).color(c)
}

fn button(key: &str, caption: &str, primary: bool, f: impl Fn(&mut State) + 'static) -> El {
    let (bg, hover) = if primary { (rgba(0x3b8f3b, 1.0), rgba(0x4caf4c, 1.0)) } else { (rgba(0x3a3f4b, 0.95), rgba(0x4b5262, 1.0)) };
    el().key(key.to_string())
        .role("button")
        .label(caption.to_string())
        .h(38.0)
        .px(18.0)
        .center()
        .radius(7.0)
        .fill(bg)
        .hover(hover)
        .press(rgba(0x2a2e36, 1.0))
        .border(1.0, rgba(0x000000, 0.35))
        .on_press(f)
        .child(label(caption, LABEL, white()))
}

fn small_button(key: &str, caption: &str, f: impl Fn(&mut State) + 'static) -> El {
    el().key(key.to_string())
        .role("button")
        .label(caption.to_string())
        .size(34.0, 30.0)
        .center()
        .radius(6.0)
        .fill(rgba(0x3a3f4b, 0.95))
        .hover(rgba(0x4b5262, 1.0))
        .on_press(f)
        .child(label(caption, LABEL, white()))
}

fn field(edit: &rune::TextEdit, key: &str, placeholder: &str, width: f32, get: fn(&mut State) -> &mut rune::TextEdit) -> El {
    text_input(edit, BODY)
        .key(key.to_string())
        .single_line()
        .placeholder(placeholder.to_string(), rgba(0x9aa0aa, 1.0))
        .caret(white(), rgba(0x4f8dff, 0.45))
        .color(white())
        .on_edit(get)
        .w(width)
        .px(10.0)
        .py(9.0)
        .radius(6.0)
        .fill(rgba(0x12151b, 0.9))
        .border(1.0, rgba(0x5a6070, 0.8))
}

fn card(children: Vec<El>) -> El {
    col().padding(22.0)
        .gap(12.0)
        .radius(12.0)
        .fill(rgba(0x16191f, 0.92))
        .border(1.0, rgba(0xffffff, 0.08))
        .shadow(10.0, 30.0, rgba(0x000000, 0.5))
        .children(children)
}

fn icon(s: &State, id: BlockId, size: f32) -> El {
    match s.icons.get(&id) {
        Some(p) => el().size(size, size).pixels(p),
        None => el().size(size, size),
    }
}

pub fn view(s: &State, cx: &mut Cx<State>) -> El {
    match s.screen {
        Screen::Title => title(s),
        Screen::Connecting => connecting(s),
        Screen::Playing => game(s, cx),
    }
}

// ---------------------------------------------------------------------------------------------
// Title

fn title(s: &State) -> El {
    let heading = col().gap(4.0).items_center().child(label("CUBETERRA", TextStyle::new(52.0, 800.0).tracking(4.0), white())).child(label(
        "Build anything. Visit New York, Tokyo, Paris and London. Play with friends.",
        BODY,
        rgba(0xd6e4ff, 0.9),
    ));
    let single = card(vec![
        label("Single player", HEAD, white()),
        row().gap(10.0).center_y().child(label("Your name", SMALL, rgba(0xb8c0cc, 1.0)).w(84.0)).child(field(&s.name, "name", "Player", 230.0, |s| &mut s.name)),
        row().gap(10.0).center_y().child(label("World", SMALL, rgba(0xb8c0cc, 1.0)).w(84.0)).child(field(&s.world_name, "world", "My World", 230.0, |s| &mut s.world_name)),
        row().gap(10.0).center_y().child(label("Seed", SMALL, rgba(0xb8c0cc, 1.0)).w(84.0)).child(field(&s.seed_text, "seed", "random (new worlds only)", 230.0, |s| &mut s.seed_text)),
        row().gap(10.0)
            .child(button("play", "Play", true, |s| s.host(false)))
            .child(button("lan", "Host on LAN", false, |s| s.host(true))),
        label("Worlds save automatically. Hosting on LAN lets friends join with your IP address.", SMALL, rgba(0x9aa3b0, 1.0)).wrap().w(330.0),
    ])
    .w(374.0);
    let multi = card(vec![
        label("Multiplayer", HEAD, white()),
        label("Join a friend's LAN game or a dedicated server.", SMALL, rgba(0x9aa3b0, 1.0)),
        row().gap(10.0).center_y().child(label("Address", SMALL, rgba(0xb8c0cc, 1.0)).w(84.0)).child(field(&s.addr, "addr", "host or host:port", 230.0, |s| &mut s.addr)),
        row().gap(10.0).child(button("join", "Join Server", true, |s| s.join())),
        label("Run your own: cargo run --release -p cubeterra-server", SMALL, rgba(0x9aa3b0, 1.0)),
    ])
    .w(374.0);
    let mut page = col().key("title")
        .w_full()
        .h_full()
        .center()
        .gap(26.0)
        .linear_gradient(std::f32::consts::PI, [(0.0, rgba(0x2f6fd1, 1.0)), (0.62, rgba(0x8cc2ff, 1.0)), (0.63, rgba(0x5f9e35, 1.0)), (1.0, rgba(0x3f6a25, 1.0))])
        .child(heading)
        .child(row().gap(20.0).items_start().child(single).child(multi));
    if !s.status.is_empty() {
        page = page.child(el().px(14.0).py(8.0).radius(8.0).fill(rgba(0x000000, 0.55)).child(label(s.status.clone(), BODY, rgba(0xffd4a8, 1.0))));
    }
    page.child(label("Built with Rune  ·  WASD move  ·  drag to look  ·  click mine  ·  right-click build", SMALL, rgba(0xffffff, 0.75)))
}

fn connecting(s: &State) -> El {
    col().w_full()
        .h_full()
        .center()
        .gap(16.0)
        .fill(rgba(0x10131a, 1.0))
        .child(label(if s.status.is_empty() { "Connecting…".to_string() } else { s.status.clone() }, HEAD, white()))
        .child(button("cancel", "Cancel", false, |s| s.leave()))
}

// ---------------------------------------------------------------------------------------------
// In game

fn key_name(e: &KeyEvent) -> Option<String> {
    Some(match &e.key {
        Key::Text(t) => t.to_lowercase(),
        Key::Left => "arrowleft".into(),
        Key::Right => "arrowright".into(),
        Key::Up => "arrowup".into(),
        Key::Down => "arrowdown".into(),
        Key::Command(c) => c.to_string(),
        _ => return None,
    })
}

/// Window-level keys: everything typed while no text field has focus.
pub fn on_key(s: &mut State, e: &KeyEvent, tx: &mut Tx) -> bool {
    if let Key::Modifier { modifier, pressed } = e.key {
        use rune::Modifier as M;
        match modifier {
            M::ShiftLeft | M::ShiftRight => s.shift = pressed,
            M::ControlLeft | M::ControlRight => s.ctrl = pressed,
            _ => {}
        }
        return s.screen == Screen::Playing;
    }
    s.shift = e.mods.shift;
    s.ctrl = e.mods.ctrl;
    let repeat = tx.key_repeat();
    match s.screen {
        Screen::Title => false,
        Screen::Connecting => {
            if e.key == Key::Escape {
                s.leave();
                return true;
            }
            false
        }
        Screen::Playing => {
            if s.panel != Panel::None {
                let close = e.key == Key::Escape || (s.panel == Panel::Inventory && key_name(e).as_deref() == Some("e")) || (s.panel == Panel::Travel && key_name(e).as_deref() == Some("m"));
                if close && s.panel != Panel::Dead {
                    s.panel = if s.panel == Panel::Settings || s.panel == Panel::Help { Panel::Pause } else { Panel::None };
                    return true;
                }
                return false;
            }
            if e.key == Key::Escape {
                s.panel = Panel::Pause;
                s.held.clear();
                return true;
            }
            if e.key == Key::Enter {
                open_chat(s, "");
                return true;
            }
            if let Key::Function(3) = e.key {
                s.show_debug = !s.show_debug;
                return true;
            }
            let Some(k) = key_name(e) else { return false };
            if !repeat {
                match k.as_str() {
                    "e" => {
                        s.panel = Panel::Inventory;
                        s.held.clear();
                        return true;
                    }
                    "m" => {
                        s.panel = Panel::Travel;
                        s.held.clear();
                        return true;
                    }
                    "t" => {
                        open_chat(s, "");
                        return true;
                    }
                    "/" => {
                        open_chat(s, "/");
                        return true;
                    }
                    "h" => {
                        s.panel = Panel::Help;
                        s.held.clear();
                        return true;
                    }
                    "f" => {
                        if s.mode == Mode::Creative {
                            s.flying = !s.flying;
                        }
                        return true;
                    }
                    " " => s.jump_pressed(),
                    d if d.len() == 1 && ("1"..="9").contains(&d) => {
                        s.selected = d.parse::<usize>().unwrap() - 1;
                        return true;
                    }
                    _ => {}
                }
            }
            s.key_down(&k);
            true
        }
    }
}

fn open_chat(s: &mut State, start: &str) {
    s.panel = Panel::Chat;
    s.held.clear();
    let mut edit = rune::TextEdit::new(start);
    edit.anchor = start.len();
    edit.focus = start.len();
    edit.focused = true;
    s.chat_edit = edit;
}

fn on_key_up(s: &mut State, e: &KeyEvent, _: &mut Tx) -> bool {
    s.shift = e.mods.shift;
    s.ctrl = e.mods.ctrl;
    if let Some(k) = key_name(e) {
        s.key_up(&k);
    }
    true
}

fn game(s: &State, cx: &mut Cx<State>) -> El {
    // The game element fills the window: its size is the window in points.
    let [w, h] = cx.layout_size("game").unwrap_or_else(|| {
        let scale = cx.env().scale.max(0.5);
        [s.screen_px.0 as f32 / scale, s.screen_px.1 as f32 / scale]
    });
    let mut root = el().key("game")
        .w_full()
        .h_full()
        .on_drag(|s: &mut State, d: &mut rune::Drag, _tx: &mut Tx| {
            if s.panel == Panel::None {
                s.drag(d.phase, d.dx, d.dy);
            } else if d.phase == Phase::Up {
                s.look = Default::default();
            }
        })
        .on_secondary_press(|s: &mut State, _p, _tx: &mut Tx| s.use_click())
        .on_wheel(|s: &mut State, e: WheelEvent, _tx: &mut Tx| {
            if s.panel == Panel::None {
                s.wheel(e.dy);
                true
            } else {
                false
            }
        })
        .on_key_up(on_key_up)
        .on_window_focus(|s: &mut State, focused: bool, _tx: &mut Tx| {
            if !focused {
                s.held.clear();
                s.look = Default::default();
            }
        });

    // Name tags over other players.
    for o in s.others.values() {
        if o.pos.distance(s.body.pos) > 64.0 {
            continue;
        }
        if let Some((x, y)) = s.project(o.pos + Vec3::Y * 2.3, w, h) {
            let tag = el().px(6.0).py(2.0).radius(4.0).fill(rgba(0x000000, 0.45)).child(label(o.name.clone(), SMALL, white()));
            root = root.child(el().absolute(x - 60.0, y - 10.0).w(120.0).center().inert().child(tag));
        }
    }

    // Crosshair.
    let bar = |w: f32, h: f32| el().size(w, h).fill(rgba(0xffffff, 0.9)).border(0.5, rgba(0x000000, 0.4));
    root = root.child(
        el().pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
            .center()
            .inert()
            .child(el().size(20.0, 20.0).child(bar(20.0, 2.0).absolute(0.0, 9.0)).child(bar(2.0, 20.0).absolute(9.0, 0.0))),
    );
    if s.hurt_flash > 0.0 {
        root = root.child(el().pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0)).fill(rgba(0xff0000, s.hurt_flash * 0.3)).inert());
    }
    root = root.child(top_bar(s)).child(chat_lines(s)).child(hotbar(s));
    if s.show_debug {
        root = root.child(debug(s));
    }
    let feet = s.body.pos.floor().as_ivec3();
    if !s.world.loaded(feet.x, feet.z) {
        root = root.child(el().pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0)).center().fill(rgba(0x0d1016, 0.9)).inert().child(label("Loading terrain…", HEAD, white())));
    }
    if s.t < s.hint_until && s.panel == Panel::None {
        root = root.child(
            el().pin(None, Some(60.0), Some(16.0), None)
                .inert()
                .child(card(controls_lines().into_iter().take(9).map(|(k, v)| row().gap(10.0).child(label(k, LABEL, rgba(0xffe08a, 1.0)).w(110.0)).child(label(v, SMALL, white()))).collect()).padding(14.0).gap(4.0)),
        );
    }
    match s.panel {
        Panel::None => {}
        Panel::Pause => root = root.child(modal(pause(s))),
        Panel::Settings => root = root.child(modal(settings(s))),
        Panel::Inventory => root = root.child(modal(inventory(s))),
        Panel::Travel => root = root.child(modal(travel(s))),
        Panel::Chat => root = root.child(chat_panel(s)),
        Panel::Dead => root = root.child(modal(dead(s)).fill(rgba(0x7a0000, 0.45))),
        Panel::Help => root = root.child(modal(help())),
    }
    root
}

fn modal(content: El) -> El {
    el().key("modal").pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0)).center().fill(rgba(0x000000, 0.45)).child(content)
}

fn top_bar(s: &State) -> El {
    let mut left = col().gap(4.0);
    if let Some((name, dist, dir)) = s.nearest_city() {
        let d = if dist > 1000.0 { format!("{:.1} km", dist / 1000.0) } else { format!("{} m", dist as i32) };
        let txt = if dist < 300.0 { format!("You are in {name}") } else { format!("{name} · {d} {dir}  (M to travel)") };
        left = left.child(el().px(10.0).py(5.0).radius(6.0).fill(rgba(0x000000, 0.4)).child(label(txt, SMALL, white())));
    }
    let mut right = col().gap(4.0).items_end();
    let mut status = format!("{} online", s.others.len() + 1);
    if s.lan
        && let Some(server) = &s.server
    {
        status = format!("{status} · LAN {}:{}", crate::local_ip(), server.addr.port());
    }
    if s.mode == Mode::Creative {
        status = format!("{status} · Creative{}", if s.flying { " (flying)" } else { "" });
    }
    right = right.child(el().px(10.0).py(5.0).radius(6.0).fill(rgba(0x000000, 0.4)).child(label(status, SMALL, white())));
    row().pin(Some(12.0), Some(10.0), Some(12.0), None).justify_between().items_start().inert().child(left).child(right)
}

fn chat_lines(s: &State) -> El {
    let mut c = col().gap(2.0).w(460.0);
    let recent: Vec<_> = s.chat.iter().rev().filter(|l| s.t - l.at < 10.0).take(8).collect();
    for l in recent.into_iter().rev() {
        let age = (s.t - l.at) as f32;
        let a = (10.0 - age).clamp(0.0, 1.0);
        let color = if l.system { rgba(0xffe08a, a) } else { rgba(0xffffff, a) };
        c = c.child(el().px(8.0).py(3.0).radius(4.0).fill(rgba(0x000000, 0.4 * a)).child(label(l.text.clone(), BODY, color).wrap()));
    }
    el().pin(Some(12.0), None, None, Some(112.0)).inert().child(c)
}

fn bar_icons(n: usize, value: f32, full: u32, empty: u32) -> El {
    let mut r = row().gap(2.0);
    for i in 0..n {
        let v = value / 2.0 - i as f32;
        let c = if v >= 1.0 {
            rgba(full, 1.0)
        } else if v >= 0.5 {
            rgba(full, 0.55)
        } else {
            rgba(empty, 0.8)
        };
        r = r.child(el().size(14.0, 12.0).radius(3.0).fill(c).border(1.0, rgba(0x000000, 0.55)));
    }
    r
}

fn slot(s: &State, stack: Option<crate::game::Stack>, selected: bool, size: f32) -> El {
    let mut sl = el().size(size, size)
        .radius(6.0)
        .center()
        .fill(rgba(0x1b1f27, 0.78))
        .border(if selected { 3.0 } else { 1.0 }, if selected { rgba(0xffffff, 0.95) } else { rgba(0x000000, 0.6) });
    if let Some(st) = stack {
        sl = sl.child(icon(s, st.id, size * 0.72));
        if st.n > 1 {
            sl = sl.child(el().absolute(size - 22.0, size - 17.0).w(18.0).child(label(st.n.to_string(), SMALL, white())));
        }
    }
    sl
}

fn hotbar(s: &State) -> El {
    let mut bar = row().gap(4.0);
    for i in 0..9 {
        bar = bar.child(slot(s, s.inv[i], i == s.selected, 46.0));
    }
    let mut c = col().gap(6.0).items_center();
    if let Some(st) = s.held_item() {
        c = c.child(el().px(8.0).py(2.0).radius(4.0).fill(rgba(0x000000, 0.35)).child(label(block::item_name(st.id), SMALL, white())));
    }
    if s.mode == Mode::Survival {
        let mut stats = row().gap(24.0).child(bar_icons(10, s.health, 0xe0323a, 0x3a1416)).child(bar_icons(10, s.hunger, 0xc98a3a, 0x3a2a14));
        if s.air < 10.0 {
            stats = stats.child(bar_icons(5, s.air, 0x7fc8ff, 0x1a2a3a));
        }
        c = c.child(stats);
    }
    c = c.child(bar);
    row().pin(Some(0.0), None, Some(0.0), Some(12.0)).justify_center().inert().child(c)
}

fn debug(s: &State) -> El {
    let p = s.body.pos;
    let lines = [
        format!("Cubeterra on Rune · {:.0} fps", s.fps),
        format!("XYZ {:.1} / {:.1} / {:.1}", p.x, p.y, p.z),
        format!("Facing {:.0}°  pitch {:.0}°", s.yaw.to_degrees(), s.pitch.to_degrees()),
        format!("Biome {}", s.biome_here()),
        format!("Time {:02}:{:02}", ((s.time_of_day * 24.0 + 6.0) as i32) % 24, (s.time_of_day * 1440.0) as i32 % 60),
        format!("Chunks {}  meshing {}", s.world.columns.len(), s.world.pending()),
        format!("Seed {}  world {}", s.seed, s.world_label),
        match s.target {
            Some(h) => format!("Looking at {} ({}, {}, {})", block::item_name(h.block), h.pos.x, h.pos.y, h.pos.z),
            None => "Looking at sky".into(),
        },
    ];
    let mut c = col().gap(1.0).px(8.0).py(6.0).radius(6.0).fill(rgba(0x000000, 0.5));
    for l in lines {
        c = c.child(label(l, SMALL, white()));
    }
    el().pin(Some(12.0), Some(44.0), None, None).inert().child(c)
}

// ---------------------------------------------------------------------------------------------
// Panels

fn pause(s: &State) -> El {
    let mode = if s.mode == Mode::Creative { "Switch to Survival" } else { "Switch to Creative" };
    card(vec![
        label("Game paused", HEAD, white()),
        button("resume", "Back to game", true, |s| s.panel = Panel::None).w_full(),
        button("travel", "Travel to a city", false, |s| s.panel = Panel::Travel).w_full(),
        button("mode", mode, false, |s| {
            let m = if s.mode == Mode::Creative { Mode::Survival } else { Mode::Creative };
            s.set_mode(m);
            s.panel = Panel::None;
        })
        .w_full(),
        button("settings", "Settings", false, |s| s.panel = Panel::Settings).w_full(),
        button("help", "Controls", false, |s| s.panel = Panel::Help).w_full(),
        button("quit", if s.server.is_some() { "Save and quit to title" } else { "Disconnect" }, false, |s| s.leave()).w_full(),
    ])
    .w(320.0)
}

fn stepper(title: &str, value: String, key: &str, minus: fn(&mut State), plus: fn(&mut State)) -> El {
    row().gap(10.0)
        .center_y()
        .child(label(title.to_string(), BODY, white()).w(150.0))
        .child(small_button(&format!("{key}-"), "−", minus))
        .child(label(value, LABEL, white()).w(70.0).center())
        .child(small_button(&format!("{key}+"), "+", plus))
}

fn settings(s: &State) -> El {
    card(vec![
        label("Settings", HEAD, white()),
        stepper(
            "View distance",
            format!("{} chunks", s.view_distance),
            "view",
            |s| {
                s.view_distance = (s.view_distance - 1).max(3);
                s.send(cubeterra_common::protocol::ToServer::ViewDistance(s.view_distance));
            },
            |s| {
                s.view_distance = (s.view_distance + 1).min(16);
                s.send(cubeterra_common::protocol::ToServer::ViewDistance(s.view_distance));
            },
        ),
        stepper("Field of view", format!("{:.0}°", s.fov), "fov", |s| s.fov = (s.fov - 5.0).max(50.0), |s| s.fov = (s.fov + 5.0).min(110.0)),
        stepper("Look speed", format!("{:.1}×", s.sensitivity), "sens", |s| s.sensitivity = (s.sensitivity - 0.1).max(0.2), |s| s.sensitivity = (s.sensitivity + 0.1).min(3.0)),
        button("back", "Done", true, |s| s.panel = Panel::Pause).w_full(),
    ])
    .w(420.0)
}

fn controls_lines() -> Vec<(&'static str, &'static str)> {
    vec![
        ("W A S D", "Walk"),
        ("Drag mouse", "Look around (or arrow keys)"),
        ("Click / hold", "Mine a block, hit a mob"),
        ("Right-click", "Place block, eat, use table, light TNT"),
        ("Space", "Jump · double-tap to fly in Creative"),
        ("Shift / Ctrl", "Sneak or fly down / sprint"),
        ("1–9 · wheel", "Choose hotbar slot"),
        ("E", "Inventory and crafting"),
        ("M", "Travel to New York, Tokyo, Paris, London"),
        ("T  or  /", "Chat and commands (/help)"),
        ("F", "Toggle flying (Creative)"),
        ("F3", "Debug info"),
        ("Esc", "Pause menu"),
    ]
}

fn help() -> El {
    let mut rows: Vec<El> = vec![label("Controls", HEAD, white())];
    for (k, v) in controls_lines() {
        rows.push(row().gap(12.0).child(label(k, LABEL, rgba(0xffe08a, 1.0)).w(120.0)).child(label(v, BODY, white())));
    }
    rows.push(label("Commands: /tp tokyo · /tp x y z · /time day · /gamemode survival · /list", SMALL, rgba(0xb8c0cc, 1.0)));
    rows.push(button("back", "Back", true, |s| s.panel = Panel::Pause));
    card(rows).w(500.0)
}

fn travel(s: &State) -> El {
    let mut rows: Vec<El> = vec![label("Travel", HEAD, white()), label("Teleport to a city. Everything you build there is saved and shared.", SMALL, rgba(0xb8c0cc, 1.0))];
    for (i, c) in CITIES.iter().enumerate() {
        let d = Vec3::new(c.x as f32, 0.0, c.z as f32).distance(s.body.pos.with_y(0.0));
        let info = col().gap(2.0).grow(1.0).shrink().child(label(c.name, LABEL, white())).child(label(c.blurb, SMALL, rgba(0xb8c0cc, 1.0)).wrap()).child(label(
            format!("{:.1} km away", d / 1000.0),
            SMALL,
            rgba(0x8fd18f, 1.0),
        ));
        rows.push(
            row().gap(12.0)
                .center_y()
                .padding(10.0)
                .radius(8.0)
                .fill(rgba(0xffffff, 0.05))
                .child(info)
                .child(button(&format!("go{i}"), "Go", true, move |s| {
                    let (x, y, z) = CITIES[i].arrival();
                    s.travel(Vec3::new(x, y, z));
                    s.say(format!("Welcome to {}!", CITIES[i].name));
                })),
        );
    }
    rows.push(row().gap(10.0).child(button("spawn", "World spawn", false, |s| {
        let p = s.spawn;
        s.travel(p);
    })).child(button("close", "Close", false, |s| s.panel = Panel::None)));
    card(rows).w(520.0)
}

fn chat_panel(s: &State) -> El {
    let mut log = col().gap(2.0);
    for l in s.chat.iter().rev().take(14).collect::<Vec<_>>().into_iter().rev() {
        log = log.child(label(l.text.clone(), BODY, if l.system { rgba(0xffe08a, 1.0) } else { white() }).wrap());
    }
    let input = text_input(&s.chat_edit, BODY)
        .key("chat")
        .single_line()
        .placeholder("Say something, or /help", rgba(0x9aa0aa, 1.0))
        .caret(white(), rgba(0x4f8dff, 0.45))
        .color(white())
        .on_edit(|s: &mut State| &mut s.chat_edit)
        .on_submit(|s: &mut State, _tx: &mut Tx| s.submit_chat())
        .on_input_key(|s: &mut State, e: &KeyEvent, tx: &mut Tx| {
            if e.key == Key::Escape {
                s.panel = Panel::None;
                tx.blur();
                return true;
            }
            false
        })
        .w_full()
        .px(10.0)
        .py(9.0)
        .radius(6.0)
        .fill(rgba(0x000000, 0.7));
    col().pin(Some(12.0), None, Some(12.0), Some(12.0)).gap(6.0).child(el().w(600.0).px(8.0).py(6.0).radius(6.0).fill(rgba(0x000000, 0.45)).child(log)).child(input)
}

fn dead(_s: &State) -> El {
    card(vec![
        label("You died!", TextStyle::new(34.0, 800.0), rgba(0xff8080, 1.0)),
        label("Your items are lost. Respawn back at the world spawn.", BODY, white()),
        button("respawn", "Respawn", true, |s| s.respawn()).w_full(),
        button("quit", "Quit to title", false, |s| s.leave()).w_full(),
    ])
    .w(360.0)
    .items_center()
}

fn inventory(s: &State) -> El {
    let hot = {
        let mut r = row().gap(4.0);
        for i in 0..9 {
            r = r.child(slot(s, s.inv[i], i == s.selected, 44.0).key(format!("hot{i}")).on_press(move |s: &mut State| s.selected = i));
        }
        r
    };
    if s.mode == Mode::Creative {
        let mut grid = row().flex_wrap().gap(4.0).w(13.0 * 48.0);
        let items: Vec<BlockId> = block::creative_palette().into_iter().chain(block::STICK..block::ITEM_END).collect();
        for id in items {
            grid = grid.child(
                el().key(format!("pick{id}"))
                    .size(44.0, 44.0)
                    .radius(6.0)
                    .center()
                    .fill(rgba(0x262b35, 0.9))
                    .hover(rgba(0x3a4252, 1.0))
                    .label(block::item_name(id).to_string())
                    .on_press(move |s: &mut State| s.pick_creative(id))
                    .child(icon(s, id, 32.0)),
            );
        }
        return card(vec![
            label("Creative inventory", HEAD, white()),
            label("Click a block to put it in your selected hotbar slot. Pick a slot below first.", SMALL, rgba(0xb8c0cc, 1.0)),
            col().child(grid).scroll(ScrollSpec { stick_bottom: false }).max_h(380.0),
            hot,
            button("close", "Done (E)", true, |s| s.panel = Panel::None),
        ]);
    }
    let mut grid = row().flex_wrap().gap(4.0).w(9.0 * 48.0);
    for i in 9..crate::game::INV {
        grid = grid.child(slot(s, s.inv[i], false, 44.0).key(format!("inv{i}")).on_press(move |s: &mut State| s.click_slot(i)));
    }
    let mut recipes = col().gap(4.0);
    for (i, r) in RECIPES.iter().enumerate() {
        let ok = s.can_craft(r);
        let have = r.inputs.iter().all(|&(id, n)| s.count(id) >= n as u32);
        if !have && !s.near_station(r.station) && r.station != Station::Hand {
            // Keep the list short: show station recipes only once you're near one or can afford them.
        }
        let mut ins = row().gap(6.0).center_y();
        for &(id, n) in r.inputs {
            ins = ins.child(icon(s, id, 20.0)).child(label(format!("{n}"), SMALL, if s.count(id) >= n as u32 { white() } else { rgba(0xff8080, 1.0) }));
        }
        let mut line = row().key(format!("r{i}"))
            .gap(8.0)
            .center_y()
            .px(8.0)
            .py(4.0)
            .radius(6.0)
            .fill(if ok { rgba(0x2f5a2f, 0.9) } else { rgba(0x262b35, 0.7) })
            .child(icon(s, r.out, 28.0))
            .child(col().w(150.0).child(label(format!("{} ×{}", block::item_name(r.out), r.count), SMALL, white())).child(label(
                if r.station == Station::Hand { String::new() } else { format!("needs {}", r.station.label()) },
                SMALL,
                if s.near_station(r.station) { rgba(0x9aa3b0, 1.0) } else { rgba(0xffb070, 1.0) },
            )))
            .child(ins);
        if ok {
            line = line.hover(rgba(0x3c7a3c, 1.0)).on_press(move |s: &mut State| s.craft(i));
        } else {
            line = line.opacity(0.6);
        }
        recipes = recipes.child(line);
    }
    card(vec![
        label("Inventory", HEAD, white()),
        row().gap(18.0)
            .items_start()
            .child(col().gap(10.0).child(label("Backpack — click an item to swap it into your hand", SMALL, rgba(0xb8c0cc, 1.0))).child(grid).child(label("Hotbar", SMALL, rgba(0xb8c0cc, 1.0))).child(hot))
            .child(col().gap(6.0).child(label("Crafting", LABEL, white())).child(recipes.scroll(ScrollSpec { stick_bottom: false }).max_h(400.0).w(390.0))),
        button("close", "Done (E)", true, |s| s.panel = Panel::None),
    ])
}
