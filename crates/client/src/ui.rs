//! Everything drawn with Rune: the title screen, the in-game HUD and the menus.

use crate::game::{Mode, Panel, Screen, Slot, State, View, compass};
use glam::Vec3;
use rune::tok::TextStyle;
use rune::{Color, Cx, El, Key, KeyEvent, Phase, PointerPress, ScrollSpec, Tx, WheelEvent, col, el, row, text, text_input};
use runecraft_common::block::{self, BlockId, Stack};
use runecraft_common::city::CITIES;
use runecraft_common::craft::RECIPES;
use runecraft_common::protocol::ContainerKind;

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
const SLOT: f32 = 44.0;

fn label(s: impl Into<String>, style: TextStyle, c: Color) -> El {
    text(s).font(style).color(c)
}

fn button(key: &str, caption: &str, primary: bool, f: impl Fn(&mut State) + 'static) -> El {
    let (bg, hover) = if primary { (rgba(0x3b8f3b, 1.0), rgba(0x4caf4c, 1.0)) } else { (rgba(0x3a3f4b, 0.95), rgba(0x4b5262, 1.0)) };
    el().key(key.to_string())
        .role("button")
        .label(caption.to_string())
        .h(36.0)
        .px(16.0)
        .center()
        .radius(7.0)
        .fill(bg)
        .hover(hover)
        .press(rgba(0x2a2e36, 1.0))
        .border(1.0, rgba(0x000000, 0.35))
        .on_press(move |s: &mut State| {
            s.sound(crate::audio::Sound::Click, None);
            f(s)
        })
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
    col().padding(20.0)
        .gap(10.0)
        .radius(12.0)
        .fill(rgba(0x16191f, 0.94))
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

fn stack_view(s: &State, st: Stack, size: f32) -> El {
    let mut e = el().size(size, size).center().child(icon(s, st.id, size * 0.72));
    if st.n > 1 {
        e = e.child(el().absolute(size - 22.0, size - 17.0).w(18.0).child(label(st.n.to_string(), SMALL, white())));
    }
    let max = block::max_durability(st.id);
    if max > 0 && st.dmg > 0 {
        let k = 1.0 - st.dmg as f32 / max as f32;
        let c = if k > 0.5 { 0x4cd14c } else if k > 0.2 { 0xe8c23a } else { 0xe04040 };
        e = e.child(el().absolute(6.0, size - 7.0).size(size - 12.0, 3.0).fill(rgba(0x000000, 0.8)).child(el().size((size - 12.0) * k, 3.0).fill(rgba(c, 1.0))));
    }
    e
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
    let heading = col().gap(4.0).items_center().child(label("RUNECRAFT", TextStyle::new(56.0, 800.0).tracking(4.0), white())).child(label(
        "Mine, build and survive. Explore New York, Tokyo, Chicago, London and more with friends.",
        BODY,
        rgba(0xd6e4ff, 0.9),
    ));
    let you = card(vec![
        label("You", HEAD, white()),
        row().gap(10.0).center_y().child(label("Name", SMALL, rgba(0xb8c0cc, 1.0)).w(60.0)).child(field(&s.name, "name", "Player", 200.0, |s| &mut s.name)),
        row().gap(8.0)
            .center_y()
            .child(label("Look", SMALL, rgba(0xb8c0cc, 1.0)).w(60.0))
            .child(small_button("skin-", "‹", |s| s.skin = s.skin.wrapping_sub(1) % 32))
            .child(skin_preview(s.skin))
            .child(small_button("skin+", "›", |s| s.skin = (s.skin + 1) % 32)),
        row().gap(8.0)
            .center_y()
            .child(label("Mode", SMALL, rgba(0xb8c0cc, 1.0)).w(60.0))
            .child(button("survival", "Survival", s.mode == Mode::Survival, |s| s.mode = Mode::Survival))
            .child(button("creative", "Creative", s.mode == Mode::Creative, |s| s.mode = Mode::Creative)),
    ])
    .w(330.0);
    let mut worlds = vec![label("Single player", HEAD, white())];
    for (i, w) in s.saved_worlds.iter().take(5).enumerate() {
        let name = w.clone();
        let name2 = w.clone();
        worlds.push(
            row().gap(8.0)
                .center_y()
                .px(10.0)
                .py(6.0)
                .radius(8.0)
                .fill(rgba(0xffffff, 0.05))
                .child(label(w.clone(), LABEL, white()).grow(1.0))
                .child(button(&format!("open{i}"), "Play", true, move |s| s.host_world(&name, false)))
                .child(button(&format!("lan{i}"), "LAN", false, move |s| s.host_world(&name2, true))),
        );
    }
    worlds.push(label("New world", LABEL, rgba(0xb8c0cc, 1.0)));
    worlds.push(row().gap(8.0).child(field(&s.world_name, "world", "World name", 180.0, |s| &mut s.world_name)).child(field(&s.seed_text, "seed", "Seed (optional)", 130.0, |s| &mut s.seed_text)));
    worlds.push(row().gap(10.0).child(button("play", "Create & play", true, |s| s.host(false))).child(button("lan", "Create on LAN", false, |s| s.host(true))));
    let single = card(worlds).w(380.0);
    let multi = card(vec![
        label("Multiplayer", HEAD, white()),
        label("Join a friend's LAN game or a server.", SMALL, rgba(0x9aa3b0, 1.0)),
        field(&s.addr, "addr", "host or host:port", 260.0, |s| &mut s.addr),
        button("join", "Join server", true, |s| s.join()),
        label("Host your own: cargo run --release -p runecraft-server", SMALL, rgba(0x9aa3b0, 1.0)).wrap().w(260.0),
    ])
    .w(300.0);
    let mut page = col().key("title")
        .w_full()
        .h_full()
        .center()
        .gap(22.0)
        .linear_gradient(std::f32::consts::PI, [(0.0, rgba(0x2f6fd1, 1.0)), (0.62, rgba(0x8cc2ff, 1.0)), (0.63, rgba(0x5f9e35, 1.0)), (1.0, rgba(0x3f6a25, 1.0))])
        .child(heading)
        .child(row().gap(16.0).items_start().child(you).child(single).child(multi));
    if !s.status.is_empty() {
        page = page.child(el().px(14.0).py(8.0).radius(8.0).fill(rgba(0x000000, 0.55)).child(label(s.status.clone(), BODY, rgba(0xffd4a8, 1.0))));
    }
    page.child(label("Built with Rune  ·  WASD move  ·  mouse look  ·  click mine  ·  right-click build  ·  E inventory", SMALL, rgba(0xffffff, 0.8)))
}

fn skin_preview(skin: u8) -> El {
    let skins = [0xf0c8a0, 0xd9a066, 0x9c6b3e, 0x5f3e24];
    let shirts = [0x3fa7d6, 0xd64545, 0x47b35a, 0xe8b23a, 0x8a55c9, 0xeeeeee, 0x333333, 0xf07fb3];
    col().w(36.0)
        .items_center()
        .child(el().size(16.0, 16.0).fill(rgba(skins[(skin % 4) as usize], 1.0)).border(1.0, rgba(0x000000, 0.4)))
        .child(el().size(20.0, 14.0).fill(rgba(shirts[((skin / 4) % 8) as usize], 1.0)))
        .child(el().size(16.0, 10.0).fill(rgba(0x2f4b7c, 1.0)))
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
// Keys

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
            let name = key_name(e);
            if s.panel != Panel::None {
                let toggle = matches!((s.panel, name.as_deref()), (Panel::Inventory | Panel::Crafting | Panel::Container, Some("e")) | (Panel::Map, Some("m")));
                if (e.key == Key::Escape || toggle) && !matches!(s.panel, Panel::Dead | Panel::Sleeping) {
                    if matches!(s.panel, Panel::Settings | Panel::Help) {
                        s.panel = Panel::Pause;
                    } else {
                        s.close_panel();
                    }
                    return true;
                }
                if matches!(s.panel, Panel::Inventory | Panel::Crafting | Panel::Container)
                    && let Some(d) = name.as_deref()
                    && d.len() == 1
                    && ("1"..="9").contains(&d)
                {
                    // Number keys swap the hovered... simplest: they still pick the hotbar slot.
                    s.select(d.parse::<usize>().unwrap() - 1);
                    return true;
                }
                return false;
            }
            if e.key == Key::Escape {
                if s.captured {
                    s.captured = false;
                }
                s.open(Panel::Pause);
                return true;
            }
            if e.key == Key::Enter {
                open_chat(s, "");
                return true;
            }
            match e.key {
                Key::Function(3) => {
                    s.show_debug = !s.show_debug;
                    return true;
                }
                Key::Function(5) => {
                    s.view = match s.view {
                        View::First => View::Back,
                        View::Back => View::Front,
                        View::Front => View::First,
                    };
                    return true;
                }
                Key::Function(1) => {
                    s.hint_until = if s.t < s.hint_until { 0.0 } else { s.t + 60.0 };
                    return true;
                }
                _ => {}
            }
            let Some(k) = name else { return false };
            if !repeat {
                match k.as_str() {
                    "e" => {
                        s.open(Panel::Inventory);
                        return true;
                    }
                    "m" => {
                        crate::map::draw(s, 256, 2, false);
                        s.open(Panel::Map);
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
                        s.open(Panel::Help);
                        return true;
                    }
                    "n" => {
                        s.show_minimap = !s.show_minimap;
                        return true;
                    }
                    "q" => {
                        s.drop_held();
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
                        s.select(d.parse::<usize>().unwrap() - 1);
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
    s.open(Panel::Chat);
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

// ---------------------------------------------------------------------------------------------
// In game

fn game(s: &State, cx: &mut Cx<State>) -> El {
    let [w, h] = cx.layout_size("game").unwrap_or_else(|| {
        let scale = cx.env().scale.max(0.5);
        [s.screen_px.0 as f32 / scale, s.screen_px.1 as f32 / scale]
    });
    let mut root = el().key("game")
        .w_full()
        .h_full()
        .on_drag(|s: &mut State, d: &mut rune::Drag, _tx: &mut Tx| {
            if s.panel == Panel::None {
                s.drag(d.phase, d.x, d.y, d.dx, d.dy);
            } else if d.phase == Phase::Up {
                s.look = Default::default();
            }
        })
        .on_pointer_move(|s: &mut State, p: PointerPress| s.pointer_at(p.x, p.y))
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
                s.captured = false;
            }
        });
    if s.mouse_look() {
        root = root.cursor(rune::Cursor::Hidden);
    }

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
    if s.view == View::First {
        let bar = |w: f32, h: f32| el().size(w, h).fill(rgba(0xffffff, 0.9)).border(0.5, rgba(0x000000, 0.4));
        root = root.child(
            el().pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
                .center()
                .inert()
                .child(el().size(20.0, 20.0).child(bar(20.0, 2.0).absolute(0.0, 9.0)).child(bar(2.0, 20.0).absolute(9.0, 0.0))),
        );
    }
    if s.hurt_flash > 0.0 {
        root = root.child(el().pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0)).fill(rgba(0xff0000, s.hurt_flash * 0.3)).inert());
    }
    if s.panel == Panel::Sleeping {
        root = root.child(el().pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0)).fill(rgba(0x05060a, 0.8)).inert());
    }
    root = root.child(top_bar(s)).child(chat_lines(s)).child(hotbar(s));
    if s.show_minimap {
        root = root.child(minimap(s));
    }
    if s.show_debug {
        root = root.child(debug(s));
    }
    let feet = s.body.pos.floor().as_ivec3();
    if !s.world.loaded(feet.x, feet.z) {
        root = root.child(el().pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0)).center().fill(rgba(0x0d1016, 0.92)).inert().child(label("Loading terrain…", HEAD, white())));
    } else if s.capture_enabled && !s.captured && s.panel == Panel::None {
        root = root.child(
            el().pin(Some(0.0), Some(0.0), Some(0.0), None)
                .h(h * 0.4)
                .center()
                .inert()
                .child(el().px(12.0).py(6.0).radius(8.0).fill(rgba(0x000000, 0.5)).child(label("Click to look around · Esc for the menu", BODY, white()))),
        );
    }
    if s.t < s.hint_until && s.panel == Panel::None {
        let lines: Vec<El> = controls_lines().into_iter().take(10).map(|(k, v)| row().gap(10.0).child(label(k, LABEL, rgba(0xffe08a, 1.0)).w(110.0)).child(label(v, SMALL, white()))).collect();
        root = root.child(el().pin(Some(12.0), Some(60.0), None, None).inert().child(card(lines).padding(14.0).gap(4.0)));
    }
    let panel = match s.panel {
        Panel::None | Panel::Chat => None,
        Panel::Pause => Some(pause(s)),
        Panel::Settings => Some(settings(s)),
        Panel::Inventory => Some(if s.mode == Mode::Creative { creative(s) } else { inventory(s) }),
        Panel::Crafting => Some(crafting(s)),
        Panel::Container => Some(container(s)),
        Panel::Map => Some(map_panel(s)),
        Panel::Dead => Some(dead(s)),
        Panel::Help => Some(help()),
        Panel::Sleeping => Some(sleeping()),
    };
    if s.panel == Panel::Chat {
        root = root.child(chat_panel(s));
    }
    if let Some(p) = panel {
        let mut m = modal(p);
        if s.panel == Panel::Dead {
            m = m.fill(rgba(0x7a0000, 0.45));
        }
        // The stack you're carrying follows the pointer.
        if let (Some(c), Some((x, y))) = (s.cursor, s.pointer) {
            m = m.child(el().absolute(x - 20.0, y - 20.0).inert().child(stack_view(s, c, 40.0)));
        }
        root = root.child(m);
    }
    root
}

fn modal(content: El) -> El {
    el().key("modal")
        .pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
        .center()
        .fill(rgba(0x000000, 0.45))
        .on_pointer_move(|s: &mut State, p: PointerPress| s.pointer = Some((p.x, p.y)))
        .child(content)
}

fn top_bar(s: &State) -> El {
    let mut left = col().gap(4.0);
    if let Some((name, dist, dir)) = s.nearest_city() {
        let d = if dist > 1000.0 { format!("{:.1} km", dist / 1000.0) } else { format!("{} m", dist as i32) };
        let txt = if dist < 300.0 { format!("You are in {name}") } else { format!("{name} · {d} {dir}  (M for the map)") };
        left = left.child(el().px(10.0).py(5.0).radius(6.0).fill(rgba(0x000000, 0.4)).child(label(txt, SMALL, white())));
    }
    let p = s.body.pos;
    left = left.child(el().px(10.0).py(4.0).radius(6.0).fill(rgba(0x000000, 0.3)).child(label(format!("{:.0}, {:.0}, {:.0} · {}", p.x, p.y, p.z, s.biome_here()), SMALL, rgba(0xdfe6ee, 1.0))));
    let mut status = format!("{} online", s.others.len() + 1);
    if s.lan
        && let Some(server) = &s.server
    {
        status = format!("{status} · LAN {}:{}", crate::local_ip(), server.addr.port());
    }
    if s.mode == Mode::Creative {
        status = format!("{status} · Creative{}", if s.flying { " (flying)" } else { "" });
    }
    if s.rain > 0.5 {
        status = format!("{status} · {}", if s.thunder { "Storm" } else { "Rain" });
    }
    let right = el().px(10.0).py(5.0).radius(6.0).fill(rgba(0x000000, 0.4)).child(label(status, SMALL, white()));
    row().pin(Some(12.0), Some(10.0), Some(12.0), None).justify_between().items_start().inert().child(left).child(right)
}

fn minimap(s: &State) -> El {
    let mut m = col().gap(4.0).items_center().child(el().size(140.0, 140.0).radius(8.0).border(2.0, rgba(0x000000, 0.6)).clip_rounded(8.0).pixels(&s.minimap));
    let heading = compass(Vec3::new(s.yaw.sin(), 0.0, -s.yaw.cos()));
    m = m.child(el().px(8.0).py(2.0).radius(4.0).fill(rgba(0x000000, 0.4)).child(label(format!("Facing {heading}"), SMALL, white())));
    el().pin(None, Some(44.0), Some(12.0), None).inert().child(m)
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
    el().pin(Some(12.0), None, None, Some(124.0)).inert().child(c)
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

fn slot_box(s: &State, stack: Option<Stack>, selected: bool, size: f32) -> El {
    let mut sl = el().size(size, size)
        .radius(6.0)
        .center()
        .fill(rgba(0x1b1f27, 0.82))
        .border(if selected { 3.0 } else { 1.0 }, if selected { rgba(0xffffff, 0.95) } else { rgba(0x000000, 0.6) });
    if let Some(st) = stack {
        sl = sl.child(stack_view(s, st, size));
    }
    sl
}

/// An interactive slot in a menu.
fn slot(s: &State, which: Slot, key: String) -> El {
    let selected = matches!(which, Slot::Inv(i) if i == s.selected);
    slot_box(s, s.slot(which), selected, SLOT)
        .key(key)
        .hover(rgba(0x3a4252, 1.0))
        .on_press(move |s: &mut State| s.click(which, false))
        .on_secondary_press(move |s: &mut State, _p, _tx: &mut Tx| s.click(which, true))
        .on_pointer_move(|s: &mut State, p: PointerPress| s.pointer = Some((p.x, p.y)))
}

fn hotbar(s: &State) -> El {
    let mut bar = row().gap(4.0);
    for i in 0..9 {
        bar = bar.child(slot_box(s, s.inv[i], i == s.selected, 46.0));
    }
    let mut c = col().gap(6.0).items_center();
    if let Some(st) = s.held_item()
        && s.t - s.selected_at < 2.5
    {
        c = c.child(el().px(8.0).py(2.0).radius(4.0).fill(rgba(0x000000, 0.35)).child(label(block::item_name(st.id), SMALL, white())));
    }
    if s.mode == Mode::Survival {
        let armor = s.armor_points();
        if armor > 0.0 {
            c = c.child(bar_icons(10, armor, 0xc8d0d8, 0x2a2e33));
        }
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
        format!("RuneCraft on Rune · {:.0} fps", s.fps),
        format!("XYZ {:.2} / {:.2} / {:.2}", p.x, p.y, p.z),
        format!("Facing {:.0}°  pitch {:.0}°", s.yaw.to_degrees(), s.pitch.to_degrees()),
        format!("Biome {}", s.biome_here()),
        format!("Time {:02}:{:02}", ((s.time_of_day * 24.0 + 6.0) as i32) % 24, (s.time_of_day * 1440.0) as i32 % 60),
        format!("Chunks {}  meshing {}  creatures {}", s.world.columns.len(), s.world.pending(), s.entities.len()),
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
    el().pin(Some(12.0), Some(80.0), None, None).inert().child(c)
}

// ---------------------------------------------------------------------------------------------
// Inventory, crafting and containers

fn grid_of(s: &State, slots: impl Iterator<Item = Slot>, per_row: usize, key: &str) -> El {
    let mut rows = col().gap(4.0);
    let all: Vec<Slot> = slots.collect();
    for (r, chunk) in all.chunks(per_row).enumerate() {
        let mut line = row().gap(4.0);
        for (c, sl) in chunk.iter().enumerate() {
            line = line.child(slot(s, *sl, format!("{key}{r}-{c}")));
        }
        rows = rows.child(line);
    }
    rows
}

fn backpack(s: &State) -> El {
    col().gap(8.0)
        .child(grid_of(s, (9..36).map(Slot::Inv), 9, "bag"))
        .child(el().h(4.0))
        .child(grid_of(s, (0..9).map(Slot::Inv), 9, "hot"))
}

fn craft_area(s: &State) -> El {
    let size = s.grid_size();
    let arrow = el().w(36.0).center().child(label("→", HEAD, white()));
    row().gap(10.0)
        .center_y()
        .child(grid_of(s, (0..size * size).map(Slot::Grid), size, "grid"))
        .child(arrow)
        .child(slot(s, Slot::Output, "out".into()).size(56.0, 56.0))
}

fn recipe_book(s: &State) -> El {
    let mut list = col().gap(4.0);
    for (i, r) in RECIPES.iter().enumerate() {
        let ok = s.book_can(r);
        let needs_table = !r.small() && s.panel != Panel::Crafting;
        let mut ins = row().gap(4.0).center_y();
        for (want, n) in r.ingredients() {
            ins = ins.child(icon(s, want.example(), 18.0)).child(label(format!("{n}"), SMALL, white()));
        }
        let mut line = row().key(format!("book{i}"))
            .gap(8.0)
            .center_y()
            .px(8.0)
            .py(3.0)
            .radius(6.0)
            .fill(if ok { rgba(0x2f5a2f, 0.9) } else { rgba(0x262b35, 0.7) })
            .child(icon(s, r.out, 24.0))
            .child(col().w(130.0).child(label(format!("{} ×{}", block::item_name(r.out), r.count), SMALL, white())).children(needs_table.then(|| label("needs a table", SMALL, rgba(0xffb070, 1.0)))))
            .child(ins);
        if ok {
            line = line.hover(rgba(0x3c7a3c, 1.0)).on_press(move |s: &mut State| s.book_craft(i));
        } else {
            line = line.opacity(0.55);
        }
        list = list.child(line);
    }
    col().gap(6.0)
        .child(label("Recipe book — click to craft", LABEL, white()))
        .child(list.scroll(ScrollSpec { stick_bottom: false }).max_h(330.0).w(330.0))
}

fn inventory(s: &State) -> El {
    let armor = col().gap(4.0).child(label("Armor", SMALL, rgba(0xb8c0cc, 1.0))).child(grid_of(s, (0..4).map(Slot::Armor), 1, "armor"));
    let top = row().gap(28.0)
        .items_start()
        .child(armor)
        .child(col().gap(6.0).child(label("Crafting", SMALL, rgba(0xb8c0cc, 1.0))).child(craft_area(s)).child(label(
            "Drag items into the grid. Right-click splits a stack, shift-click moves it.",
            SMALL,
            rgba(0x9aa3b0, 1.0),
        ).wrap().w(260.0)));
    card(vec![
        label("Inventory", HEAD, white()),
        row().gap(22.0).items_start().child(col().gap(14.0).child(top).child(backpack(s))).child(recipe_book(s)),
        button("close", "Done (E)", true, |s| s.close_panel()),
    ])
}

fn crafting(s: &State) -> El {
    card(vec![
        label("Crafting table", HEAD, white()),
        row().gap(22.0).items_start().child(col().gap(14.0).child(craft_area(s)).child(backpack(s))).child(recipe_book(s)),
        button("close", "Done (E)", true, |s| s.close_panel()),
    ])
}

fn container(s: &State) -> El {
    let Some(c) = &s.container else {
        return card(vec![label("Opening…", HEAD, white())]);
    };
    let top = match c.kind {
        ContainerKind::Chest => col().gap(6.0).child(label("Chest", HEAD, white())).child(grid_of(s, (0..c.slots.len()).map(Slot::Box), 9, "chest")),
        ContainerKind::Furnace => {
            let flame = el().size(20.0, 20.0).radius(4.0).fill(rgba(0x2a2e36, 1.0)).child(el().size(20.0, 20.0 * c.burn).fill(rgba(0xff8a1e, 1.0)));
            let progress = el().size(60.0, 8.0).radius(4.0).fill(rgba(0x2a2e36, 1.0)).child(el().size(60.0 * c.progress, 8.0).radius(4.0).fill(rgba(0xffffff, 0.9)));
            col().gap(8.0).child(label("Furnace", HEAD, white())).child(
                row().gap(16.0)
                    .center_y()
                    .child(col().gap(6.0).items_center().child(slot(s, Slot::Box(0), "fin".into())).child(flame).child(slot(s, Slot::Box(1), "ffuel".into())))
                    .child(progress)
                    .child(slot(s, Slot::Box(2), "fout".into()).size(56.0, 56.0))
                    .child(label("Top: what to cook or smelt.\nBottom: fuel (coal, wood, planks).", SMALL, rgba(0x9aa3b0, 1.0)).wrap().w(200.0)),
            )
        }
    };
    card(vec![top, label("Your items", SMALL, rgba(0xb8c0cc, 1.0)), backpack(s), button("close", "Done (E)", true, |s| s.close_panel())])
}

fn creative(s: &State) -> El {
    let q = s.search.text.trim().to_lowercase();
    let items: Vec<BlockId> = block::creative_palette().into_iter().filter(|&id| q.is_empty() || block::item_name(id).to_lowercase().contains(&q)).collect();
    let mut grid = row().flex_wrap().gap(4.0).w(14.0 * 48.0);
    for id in items {
        grid = grid.child(
            el().key(format!("pick{id}"))
                .size(SLOT, SLOT)
                .radius(6.0)
                .center()
                .fill(rgba(0x262b35, 0.9))
                .hover(rgba(0x3a4252, 1.0))
                .label(block::item_name(id).to_string())
                .on_press(move |s: &mut State| s.pick_creative(id))
                .on_pointer_move(|s: &mut State, p: PointerPress| s.pointer = Some((p.x, p.y)))
                .child(icon(s, id, 32.0)),
        );
    }
    let trash = el().key("trash")
        .size(SLOT, SLOT)
        .radius(6.0)
        .center()
        .fill(rgba(0x5a2020, 0.9))
        .hover(rgba(0x7a3030, 1.0))
        .on_press(|s: &mut State| s.trash_cursor())
        .child(label("🗑", BODY, white()));
    card(vec![
        row().gap(12.0).center_y().child(label("Creative", HEAD, white())).child(field(&s.search, "search", "Search blocks and items", 260.0, |s| &mut s.search)),
        label("Click to pick up one (shift: a full stack), then click a slot to place it.", SMALL, rgba(0x9aa3b0, 1.0)),
        col().child(grid).scroll(ScrollSpec { stick_bottom: false }).max_h(300.0),
        row().gap(10.0).items_end().child(backpack(s)).child(col().gap(4.0).items_center().child(trash).child(label("Trash", SMALL, rgba(0xb8c0cc, 1.0)))),
        button("close", "Done (E)", true, |s| s.close_panel()),
    ])
}

// ---------------------------------------------------------------------------------------------
// Other panels

fn pause(s: &State) -> El {
    let mode = if s.mode == Mode::Creative { "Switch to Survival" } else { "Switch to Creative" };
    card(vec![
        label("Game paused", HEAD, white()),
        button("resume", "Back to game", true, |s| s.panel = Panel::None).w_full(),
        button("map", "Map and travel", false, |s| {
            crate::map::draw(s, 256, 2, false);
            s.panel = Panel::Map;
        })
        .w_full(),
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
        .child(label(value, LABEL, white()).w(80.0).center())
        .child(small_button(&format!("{key}+"), "+", plus))
}

fn toggle(title: &str, on: bool, key: &str, f: fn(&mut State)) -> El {
    row().gap(10.0)
        .center_y()
        .child(label(title.to_string(), BODY, white()).w(150.0))
        .child(button(key, if on { "On" } else { "Off" }, on, move |s| f(s)).w(80.0))
}

fn settings(s: &State) -> El {
    let mut rows = vec![
        label("Settings", HEAD, white()),
        stepper(
            "View distance",
            format!("{} chunks", s.view_distance),
            "view",
            |s| {
                s.view_distance = (s.view_distance - 1).max(3);
                s.send(runecraft_common::protocol::ToServer::ViewDistance(s.view_distance));
            },
            |s| {
                s.view_distance = (s.view_distance + 1).min(16);
                s.send(runecraft_common::protocol::ToServer::ViewDistance(s.view_distance));
            },
        ),
        stepper("Field of view", format!("{:.0}°", s.fov), "fov", |s| s.fov = (s.fov - 5.0).max(50.0), |s| s.fov = (s.fov + 5.0).min(110.0)),
        stepper("Mouse speed", format!("{:.1}×", s.sensitivity), "sens", |s| s.sensitivity = (s.sensitivity - 0.1).max(0.2), |s| s.sensitivity = (s.sensitivity + 0.1).min(3.0)),
    ];
    if crate::capture::supported() {
        rows.push(toggle("Mouse capture", s.capture_enabled, "capture", |s| s.capture_enabled = !s.capture_enabled));
    } else {
        rows.push(label("Mouse capture isn't available on this display (Wayland), so drag to look.", SMALL, rgba(0x9aa3b0, 1.0)).wrap().w(380.0));
    }
    rows.push(toggle("Sound", s.sound_on, "sound", |s| s.sound_on = !s.sound_on));
    rows.push(toggle("Music", s.music_on, "music", |s| {
        s.music_on = !s.music_on;
        s.audio.set_music(s.music_on);
    }));
    rows.push(toggle("Minimap", s.show_minimap, "minimap", |s| s.show_minimap = !s.show_minimap));
    rows.push(button("back", "Done", true, |s| s.panel = Panel::Pause).w_full());
    card(rows).w(440.0)
}

fn controls_lines() -> Vec<(&'static str, &'static str)> {
    vec![
        ("W A S D", "Walk (Ctrl to sprint, Shift to sneak)"),
        ("Mouse", "Look around (click to grab, Esc to let go)"),
        ("Left click", "Mine (hold), attack"),
        ("Right click", "Place, use doors, beds, chests, eat, shoot"),
        ("Space", "Jump, swim up, climb · double-tap to fly"),
        ("1–9, wheel", "Choose hotbar slot"),
        ("E", "Inventory and crafting"),
        ("Q", "Drop item (Ctrl+Q: whole stack)"),
        ("M", "Map: travel to the cities"),
        ("T, /", "Chat and commands"),
        ("F5", "First / third person"),
        ("N", "Minimap"),
        ("F3", "Debug info"),
        ("Esc", "Pause, settings, quit"),
    ]
}

fn help() -> El {
    let mut rows: Vec<El> = vec![label("Controls", HEAD, white())];
    for (k, v) in controls_lines() {
        rows.push(row().gap(12.0).child(label(k, LABEL, rgba(0xffe08a, 1.0)).w(120.0)).child(label(v, BODY, white())));
    }
    rows.push(label("Commands: /tp tokyo · /tp chicago · /time day · /weather rain · /give diamond 5 · /gamemode creative", SMALL, rgba(0xb8c0cc, 1.0)).wrap().w(480.0));
    rows.push(button("back", "Back", true, |s| s.panel = Panel::Pause));
    card(rows).w(520.0)
}

fn map_panel(s: &State) -> El {
    let p = s.body.pos;
    let mut cities = col().gap(6.0);
    for (i, c) in CITIES.iter().enumerate() {
        let d = Vec3::new(c.x as f32 - p.x, 0.0, c.z as f32 - p.z);
        let info = col().gap(1.0).grow(1.0).shrink().child(label(c.name, LABEL, white())).child(label(format!("{:.1} km {}", d.length() / 1000.0, compass(d)), SMALL, rgba(0x8fd18f, 1.0)));
        cities = cities.child(
            row().gap(10.0)
                .center_y()
                .px(10.0)
                .py(5.0)
                .radius(8.0)
                .fill(rgba(0xffffff, 0.05))
                .child(info)
                .child(button(&format!("go{i}"), "Go", true, move |s| {
                    let (x, y, z) = CITIES[i].arrival();
                    s.travel(Vec3::new(x, y, z));
                    s.say(format!("Welcome to {}! {}", CITIES[i].name, CITIES[i].blurb));
                })),
        );
    }
    cities = cities.child(button("spawn", "Back to your spawn point", false, |s| {
        let p = s.spawn;
        s.travel(p);
    }));
    let map = col().gap(6.0)
        .child(el().size(384.0, 384.0).radius(8.0).clip_rounded(8.0).border(2.0, rgba(0x000000, 0.6)).pixels(&s.map))
        .child(label(format!("You: {:.0}, {:.0}, {:.0} · 512 × 512 blocks around you · north is up", p.x, p.y, p.z), SMALL, rgba(0xb8c0cc, 1.0)));
    card(vec![
        label("Map", HEAD, white()),
        row().gap(18.0).items_start().child(map).child(col().gap(8.0).w(300.0).child(label("Travel to a city", LABEL, white())).child(cities.scroll(ScrollSpec { stick_bottom: false }).max_h(380.0))),
        button("close", "Close (M)", true, |s| s.close_panel()),
    ])
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
        label("Your items dropped where you fell. Go back and pick them up!", BODY, white()),
        button("respawn", "Respawn", true, |s| s.respawn()).w_full(),
        button("quit", "Quit to title", false, |s| s.leave()).w_full(),
    ])
    .w(380.0)
    .items_center()
}

fn sleeping() -> El {
    card(vec![
        label("Sleeping…", HEAD, white()),
        label("The night passes when everyone is in bed.", BODY, white()),
        button("wake", "Leave bed", false, |s| s.leave_bed()),
    ])
    .items_center()
}
