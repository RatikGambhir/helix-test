//! Every texture in the game is painted here in code: a 256×256 atlas of 16×16 tiles, plus
//! inventory icons drawn from it.

use runecraft_common::block::{self, BlockId, tile as t};
use runecraft_common::noise::{hash3, unit};

pub const TILE: usize = 16;
pub const PER_ROW: usize = 16;
pub const ATLAS: usize = TILE * PER_ROW;

pub struct Atlas {
    pub rgba: Vec<u8>,
}

type Rgb = [f32; 3];

fn hex(c: u32) -> Rgb {
    [((c >> 16) & 255) as f32, ((c >> 8) & 255) as f32, (c & 255) as f32]
}

fn mul(c: Rgb, k: f32) -> Rgb {
    [c[0] * k, c[1] * k, c[2] * k]
}

fn mix(a: Rgb, b: Rgb, k: f32) -> Rgb {
    [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k, a[2] + (b[2] - a[2]) * k]
}

struct Painter {
    px: [[f32; 4]; 256],
    seed: u32,
}

impl Painter {
    fn new(seed: u32) -> Painter {
        Painter { px: [[0.0; 4]; 256], seed }
    }
    fn r(&self, x: i32, y: i32, salt: i32) -> f32 {
        unit(hash3(self.seed as u64 * 7919 + 17, x, y, salt))
    }
    fn set(&mut self, x: i32, y: i32, c: Rgb, a: f32) {
        if (0..16).contains(&x) && (0..16).contains(&y) {
            self.px[(y * 16 + x) as usize] = [c[0], c[1], c[2], a];
        }
    }
    fn get(&self, x: i32, y: i32) -> Rgb {
        let p = self.px[((y & 15) * 16 + (x & 15)) as usize];
        [p[0], p[1], p[2]]
    }
    /// Base colour with per-pixel brightness noise.
    fn noise(&mut self, c: u32, amount: f32) {
        let base = hex(c);
        for y in 0..16 {
            for x in 0..16 {
                let k = 1.0 + (self.r(x, y, 0) - 0.5) * amount;
                self.set(x, y, mul(base, k), 1.0);
            }
        }
    }
    fn specks(&mut self, c: u32, chance: f32, salt: i32) {
        for y in 0..16 {
            for x in 0..16 {
                if self.r(x, y, salt) < chance {
                    let k = 0.85 + self.r(x, y, salt + 1) * 0.3;
                    self.set(x, y, mul(hex(c), k), 1.0);
                }
            }
        }
    }
    /// Blobs of colour, like ore veins.
    fn blobs(&mut self, c: u32, count: i32, salt: i32) {
        for i in 0..count {
            let cx = (self.r(i, 0, salt) * 13.0) as i32 + 1;
            let cy = (self.r(i, 1, salt) * 13.0) as i32 + 1;
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1), (-1, 0), (0, -1)] {
                if self.r(cx + dx, cy + dy, salt + 9) < 0.8 {
                    let k = 0.8 + self.r(cx + dx, cy + dy, salt + 3) * 0.4;
                    self.set(cx + dx, cy + dy, mul(hex(c), k), 1.0);
                }
            }
        }
    }
    fn frame(&mut self, c: u32, inset: i32) {
        for i in inset..16 - inset {
            for (x, y) in [(i, inset), (i, 15 - inset), (inset, i), (15 - inset, i)] {
                self.set(x, y, hex(c), 1.0);
            }
        }
    }
    fn holes(&mut self, chance: f32) {
        for y in 0..16 {
            for x in 0..16 {
                if self.r(x, y, 77) < chance {
                    self.px[(y * 16 + x) as usize][3] = 0.0;
                }
            }
        }
    }
    fn clear(&mut self) {
        self.px = [[0.0; 4]; 256];
    }
    fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, c: Rgb) {
        let n = ((x1 - x0).abs().max((y1 - y0).abs()) * 2.0) as i32 + 1;
        for i in 0..=n {
            let k = i as f32 / n as f32;
            self.set((x0 + (x1 - x0) * k).round() as i32, (y0 + (y1 - y0) * k).round() as i32, c, 1.0);
        }
    }
    fn bevel(&mut self, c: u32) {
        self.noise(c, 0.08);
        let base = hex(c);
        for i in 0..16 {
            self.set(i, 0, mul(base, 1.25), 1.0);
            self.set(0, i, mul(base, 1.2), 1.0);
            self.set(i, 15, mul(base, 0.7), 1.0);
            self.set(15, i, mul(base, 0.75), 1.0);
        }
    }
    fn bricks(&mut self, brick: u32, mortar: u32, bw: i32, bh: i32) {
        for y in 0..16 {
            for x in 0..16 {
                let row = y / bh;
                let off = if row % 2 == 0 { 0 } else { bw / 2 };
                let is_mortar = y % bh == bh - 1 || (x + off) % bw == bw - 1;
                let c = if is_mortar { hex(mortar) } else { mul(hex(brick), 0.88 + self.r(x / 2 + row * 7, row, 5) * 0.2 + self.r(x, y, 6) * 0.08) };
                self.set(x, y, c, 1.0);
            }
        }
    }
    fn log_side(&mut self, c: u32, dark: u32) {
        for y in 0..16 {
            for x in 0..16 {
                let stripe = (self.r(x, 0, 3) - 0.5) * 0.3;
                let k = 1.0 + stripe + (self.r(x, y, 4) - 0.5) * 0.12;
                let col = if self.r(x, y / 3, 8) < 0.12 { hex(dark) } else { mul(hex(c), k) };
                self.set(x, y, col, 1.0);
            }
        }
    }
    fn log_top(&mut self, bark: u32, wood: u32) {
        for y in 0..16 {
            for x in 0..16 {
                let (dx, dy) = (x as f32 - 7.5, y as f32 - 7.5);
                let d = (dx * dx + dy * dy).sqrt();
                let edge = x == 0 || y == 0 || x == 15 || y == 15;
                let c = if edge { hex(bark) } else { mul(hex(wood), if (d as i32) % 3 == 0 { 0.82 } else { 1.0 }) };
                self.set(x, y, c, 1.0);
            }
        }
    }
    fn wool(&mut self, c: u32) {
        self.noise(c, 0.1);
        for y in 0..16 {
            for x in 0..16 {
                if (x + y * 3) % 5 == 0 {
                    let p = self.get(x, y);
                    self.set(x, y, mul(p, 0.9), 1.0);
                }
            }
        }
    }
    fn window(&mut self, glass: u32, frame: u32, glint: bool) {
        for y in 0..16 {
            for x in 0..16 {
                let g = hex(glass);
                let k = 0.85 + (y as f32 / 16.0) * 0.3;
                self.set(x, y, mul(g, k), 1.0);
            }
        }
        if glint {
            for i in 3..9 {
                self.set(i, 12 - i, mul(hex(glass), 1.6), 1.0);
                self.set(i + 1, 12 - i, mul(hex(glass), 1.4), 1.0);
            }
        }
        for i in 0..16 {
            self.set(i, 0, hex(frame), 1.0);
            self.set(0, i, hex(frame), 1.0);
            self.set(i, 8, hex(frame), 1.0);
            self.set(8, i, hex(frame), 1.0);
        }
    }
    /// 0 pick, 2 axe, 3 shovel, 4 hoe.
    fn sprite_kind(&mut self, head: u32, kind: u8) {
        self.clear();
        let handle = hex(0x7a5a32);
        if kind >= 2 {
            self.line(3.0, 13.0, 11.0, 5.0, handle);
            self.line(4.0, 13.0, 12.0, 5.0, mul(handle, 0.8));
            match kind {
                2 => {
                    for y in 2..8 {
                        for x in 9..14 {
                            if x - 9 <= y {
                                self.set(x, y, mul(hex(head), 0.85 + (y as f32) * 0.03), 1.0);
                            }
                        }
                    }
                }
                3 => {
                    for y in 1..6 {
                        for x in 10..15 {
                            self.set(x, y, mul(hex(head), 0.9 + (x as f32 - 10.0) * 0.04), 1.0);
                        }
                    }
                }
                _ => {
                    self.line(8.0, 3.0, 13.0, 3.0, hex(head));
                    self.line(8.0, 4.0, 12.0, 4.0, mul(hex(head), 0.8));
                }
            }
            return;
        }
        self.line(3.0, 13.0, 11.0, 5.0, handle);
        self.line(4.0, 13.0, 12.0, 5.0, mul(handle, 0.8));
        self.line(5.0, 2.0, 13.0, 10.0, hex(head));
        self.line(6.0, 2.0, 14.0, 10.0, mul(hex(head), 0.8));
        self.line(6.0, 3.0, 12.0, 3.0, hex(head));
        self.line(12.0, 3.0, 12.0, 9.0, hex(head));
    }
    fn face(&mut self, skin: u32, eyes: u32, hair: Option<u32>) {
        self.noise(skin, 0.06);
        if let Some(h) = hair {
            for x in 0..16 {
                for y in 0..4 {
                    self.set(x, y, mul(hex(h), 0.9 + self.r(x, y, 3) * 0.2), 1.0);
                }
                self.set(x, 4, hex(h), if x < 3 || x > 12 { 1.0 } else { 0.0 });
            }
            for x in 0..16 {
                if x < 3 || x > 12 {
                    self.set(x, 4, hex(h), 1.0);
                } else {
                    self.set(x, 4, mul(hex(skin), 0.95), 1.0);
                }
            }
        }
        for (x, y) in [(4, 7), (5, 7), (10, 7), (11, 7)] {
            self.set(x, y, hex(0xffffff), 1.0);
        }
        self.set(5, 7, hex(eyes), 1.0);
        self.set(10, 7, hex(eyes), 1.0);
        for x in 6..10 {
            self.set(x, 11, mul(hex(skin), 0.6), 1.0);
        }
    }
}

fn paint(id: u8) -> Painter {
    let mut p = Painter::new(id as u32);
    match id {
        t::STONE => {
            p.noise(0x7d7d7d, 0.22);
            p.specks(0x6a6a6a, 0.12, 2);
        }
        t::DIRT => {
            p.noise(0x7b5739, 0.25);
            p.specks(0x5e412a, 0.1, 2);
            p.specks(0x8f6a48, 0.05, 4);
        }
        t::GRASS_TOP => {
            p.noise(0x5f9e35, 0.25);
            p.specks(0x4d8a2a, 0.15, 2);
        }
        t::GRASS_SIDE => {
            p.noise(0x7b5739, 0.25);
            p.specks(0x5e412a, 0.1, 2);
            for x in 0..16 {
                let depth = 3 + (p.r(x, 0, 9) * 2.5) as i32;
                for y in 0..depth {
                    let k = 0.85 + p.r(x, y, 10) * 0.3;
                    p.set(x, y, mul(hex(0x5f9e35), k), 1.0);
                }
            }
        }
        t::SAND => {
            p.noise(0xdbd08f, 0.12);
            p.specks(0xc9bc7b, 0.1, 3);
        }
        t::GRAVEL => {
            p.noise(0x857f7a, 0.2);
            p.blobs(0x9c9690, 8, 3);
            p.blobs(0x5f5a56, 6, 5);
        }
        t::WATER => {
            for y in 0..16 {
                for x in 0..16 {
                    let wave = ((x as f32 * 0.8 + y as f32 * 0.4).sin() * 0.5 + 0.5) * 0.15;
                    let k = 0.92 + wave + (p.r(x, y, 1) - 0.5) * 0.06;
                    p.set(x, y, mul(hex(0x3466d4), k), 0.68);
                }
            }
        }
        t::BEDROCK => {
            p.noise(0x555555, 0.5);
            p.specks(0x222222, 0.25, 2);
        }
        t::OAK_LOG => p.log_side(0x6b5132, 0x4e3a22),
        t::OAK_LOG_TOP => p.log_top(0x5e4529, 0xb38d55),
        t::BIRCH_LOG => {
            p.noise(0xdcd9cf, 0.08);
            for i in 0..7 {
                let (x, y) = ((p.r(i, 0, 3) * 13.0) as i32, (p.r(i, 1, 3) * 15.0) as i32);
                for dx in 0..3 {
                    p.set(x + dx, y, hex(0x34312c), 1.0);
                }
            }
        }
        t::BIRCH_LOG_TOP => p.log_top(0xdcd9cf, 0xc9b27e),
        t::SPRUCE_LOG => p.log_side(0x4b3521, 0x33240f),
        t::SPRUCE_LOG_TOP => p.log_top(0x3d2a17, 0x8a6a42),
        t::OAK_LEAVES => {
            p.noise(0x3e8a2b, 0.35);
            p.holes(0.18);
        }
        t::BIRCH_LEAVES => {
            p.noise(0x6ba03f, 0.3);
            p.holes(0.18);
        }
        t::SPRUCE_LEAVES => {
            p.noise(0x2e5c36, 0.3);
            p.holes(0.15);
        }
        t::CHERRY_LEAVES => {
            p.noise(0xf3a9c6, 0.22);
            p.specks(0xfcd5e3, 0.12, 5);
            p.holes(0.15);
        }
        t::PLANKS => {
            for y in 0..16 {
                for x in 0..16 {
                    let plank = y / 4;
                    let seam = y % 4 == 3 || (x + plank * 5) % 16 == 0;
                    let k = 0.92 + p.r(x / 4, plank, 2) * 0.12 + (p.r(x, y, 3) - 0.5) * 0.08;
                    p.set(x, y, if seam { hex(0x7a5c36) } else { mul(hex(0xb48c56), k) }, 1.0);
                }
            }
        }
        t::COBBLE => {
            for y in 0..16 {
                for x in 0..16 {
                    // Rough cells: nearest of a few jittered centres.
                    let mut best = (99.0f32, 99.0f32, 0);
                    for i in 0..9 {
                        let cx = (i % 3) as f32 * 5.5 + p.r(i, 0, 4) * 4.0;
                        let cy = (i / 3) as f32 * 5.5 + p.r(i, 1, 4) * 4.0;
                        let d = (x as f32 - cx).hypot(y as f32 - cy);
                        if d < best.0 {
                            best = (d, best.0, i);
                        } else if d < best.1 {
                            best.1 = d;
                        }
                    }
                    let mortar = best.1 - best.0 < 1.0;
                    let k = 0.85 + p.r(best.2, 5, 6) * 0.3 + (p.r(x, y, 7) - 0.5) * 0.1;
                    p.set(x, y, if mortar { hex(0x4f4f4f) } else { mul(hex(0x8a8a8a), k) }, 1.0);
                }
            }
        }
        t::GLASS => {
            p.clear();
            p.frame(0xdbeefa, 0);
            for i in 3..7 {
                p.set(i, 9 - i, hex(0xffffff), 0.8);
            }
            for i in 9..12 {
                p.set(i, 21 - i, hex(0xffffff), 0.6);
            }
        }
        t::BRICK => p.bricks(0x9b4a39, 0xb5ad9f, 8, 4),
        t::STONE_BRICK => p.bricks(0x8a8a88, 0x5b5b5a, 16, 8),
        t::COAL_ORE | t::IRON_ORE | t::GOLD_ORE | t::DIAMOND_ORE => {
            p.noise(0x7d7d7d, 0.22);
            let c = match id {
                t::COAL_ORE => 0x262626,
                t::IRON_ORE => 0xd9ad8e,
                t::GOLD_ORE => 0xf6d63b,
                _ => 0x5fe8e0,
            };
            p.blobs(c, 5, 11);
        }
        t::SNOW => p.noise(0xf1f6f9, 0.05),
        t::ICE => {
            p.noise(0x9fc6f4, 0.1);
            for i in 0..10 {
                p.set(i + 3, i, hex(0xd6e8ff), 1.0);
            }
        }
        t::CACTUS_SIDE => {
            for y in 0..16 {
                for x in 0..16 {
                    let rib = x % 4 == 1;
                    let k = 0.9 + (p.r(x, y, 2) - 0.5) * 0.15;
                    p.set(x, y, mul(hex(if rib { 0x4d9338 } else { 0x3c7a2b }), k), 1.0);
                    if rib && y % 4 == 2 {
                        p.set(x, y, hex(0xe6e0b0), 1.0);
                    }
                }
            }
        }
        t::CACTUS_TOP => {
            p.noise(0x4d9338, 0.15);
            p.frame(0x3c7a2b, 0);
        }
        t::CLAY => p.noise(0x9fa5b1, 0.08),
        t::SANDSTONE => {
            p.noise(0xd8cb8a, 0.08);
            for x in 0..16 {
                p.set(x, 5, hex(0xc5b674), 1.0);
                p.set(x, 11, hex(0xc5b674), 1.0);
            }
        }
        t::SANDSTONE_TOP => p.noise(0xdccf90, 0.06),
        t::TALL_GRASS | t::FLOWER_RED | t::FLOWER_YELLOW => {
            p.clear();
            let green = hex(0x4f8f30);
            for i in 0..6 {
                let x0 = 2.0 + i as f32 * 2.3;
                let lean = (p.r(i, 0, 1) - 0.5) * 4.0;
                let top = if id == t::TALL_GRASS { 3.0 + p.r(i, 1, 1) * 6.0 } else { 8.0 };
                if id != t::TALL_GRASS && i % 2 == 1 {
                    continue;
                }
                p.line(x0, 15.0, x0 + lean, top, mul(green, 0.85 + p.r(i, 2, 1) * 0.3));
            }
            if id != t::TALL_GRASS {
                let petal = if id == t::FLOWER_RED { 0xd92b2b } else { 0xf5d531 };
                for (dx, dy) in [(0, -1), (-1, 0), (1, 0), (0, 1), (0, 0)] {
                    p.set(7 + dx, 5 + dy, if (dx, dy) == (0, 0) { hex(0x3a2a10) } else { hex(petal) }, 1.0);
                }
                p.line(7.0, 7.0, 7.0, 15.0, green);
            }
        }
        t::TABLE_TOP => {
            p.px = paint(t::PLANKS).px;
            p.frame(0x5e4529, 0);
            p.line(3.0, 3.0, 12.0, 12.0, hex(0x5e4529));
        }
        t::TABLE_SIDE => {
            p.px = paint(t::PLANKS).px;
            for x in 0..16 {
                p.set(x, 0, hex(0x5e4529), 1.0);
                p.set(x, 1, hex(0x5e4529), 1.0);
            }
            p.line(3.0, 5.0, 6.0, 12.0, hex(0x777777));
            p.line(9.0, 5.0, 12.0, 5.0, hex(0x777777));
            p.line(10.0, 5.0, 10.0, 12.0, hex(0x6b4f2d));
        }
        t::FURNACE_SIDE => {
            p.px = paint(t::COBBLE).px;
        }
        t::FURNACE_FRONT => {
            p.px = paint(t::COBBLE).px;
            for y in 8..14 {
                for x in 4..12 {
                    let glow = if y > 10 { 0xff8a1e } else { 0x1d1d1d };
                    p.set(x, y, hex(glow), 1.0);
                }
            }
        }
        t::BOOKSHELF => {
            p.px = paint(t::PLANKS).px;
            let spines = [0x8a2b2b, 0x2b4f8a, 0x2f7a3a, 0x8a7a2b, 0x5a2b8a, 0x2b7a7a];
            for shelf in 0..2 {
                for x in 1..15 {
                    let c = spines[(p.r(x, shelf, 3) * 6.0) as usize];
                    for y in (shelf * 8 + 1)..(shelf * 8 + 7) {
                        p.set(x, y, mul(hex(c), 0.85 + p.r(x, y, 1) * 0.2), 1.0);
                    }
                }
            }
        }
        t::GLOWSTONE => {
            p.noise(0xf3c04d, 0.3);
            p.blobs(0xfff2b3, 7, 4);
            p.specks(0xa86e1d, 0.08, 6);
        }
        t::LANTERN => {
            p.noise(0xffd66b, 0.15);
            p.frame(0x2e2a26, 0);
            p.frame(0x3d3833, 1);
            for y in 5..11 {
                for x in 5..11 {
                    p.set(x, y, hex(0xfff6d6), 1.0);
                }
            }
        }
        t::OBSIDIAN => {
            p.noise(0x1a1024, 0.3);
            p.specks(0x4b2f6b, 0.08, 3);
        }
        t::TNT_SIDE => {
            p.noise(0xc23a2a, 0.1);
            for y in 5..11 {
                for x in 0..16 {
                    p.set(x, y, hex(0xeeeeee), 1.0);
                }
            }
            for x in [3, 4, 5, 7, 9, 10, 11] {
                p.set(x, 7, hex(0x222222), 1.0);
            }
            for y in 7..10 {
                p.set(4, y, hex(0x222222), 1.0);
                p.set(10, y, hex(0x222222), 1.0);
                p.set(7, y, hex(0x222222), 1.0);
            }
        }
        t::TNT_TOP => {
            p.noise(0xc23a2a, 0.1);
            p.frame(0x9a2a1e, 0);
            p.set(7, 7, hex(0x333333), 1.0);
            p.set(8, 8, hex(0x333333), 1.0);
        }
        t::LAVA => {
            for y in 0..16 {
                for x in 0..16 {
                    let s = ((x as f32 * 0.7).sin() + (y as f32 * 0.5 + x as f32 * 0.2).cos()) * 0.25 + 0.5;
                    p.set(x, y, mix(hex(0xd8400c), hex(0xffc23a), s), 1.0);
                }
            }
        }
        _ if (t::WOOL..t::WOOL + 6).contains(&id) => {
            p.wool([0xe9ecec, 0xb02e26, 0x3c44aa, 0x5e7c16, 0xf8c627, 0x1d1d21][(id - t::WOOL) as usize]);
        }
        _ if (t::CONCRETE..t::CONCRETE + 3).contains(&id) => {
            p.noise([0xd8dadc, 0x8b8e92, 0x3e4146][(id - t::CONCRETE) as usize], 0.05);
        }
        t::ASPHALT => {
            p.noise(0x38393c, 0.18);
            p.specks(0x4a4b4e, 0.08, 2);
        }
        t::ROAD_LINE => {
            p.px = paint(t::ASPHALT).px;
            for y in 0..16 {
                for x in 6..10 {
                    p.set(x, y, mul(hex(0xf2c230), 0.9 + p.r(x, y, 3) * 0.15), 1.0);
                }
            }
        }
        t::ROAD_WHITE => {
            p.noise(0xe4e4e0, 0.08);
            p.specks(0x55565a, 0.06, 4);
        }
        t::SIDEWALK => {
            p.noise(0xa9a8a3, 0.08);
            for i in 0..16 {
                p.set(i, 0, hex(0x86857f), 1.0);
                p.set(0, i, hex(0x86857f), 1.0);
                p.set(i, 8, hex(0x8f8e88), 1.0);
            }
        }
        t::WINDOW_BLUE => p.window(0x4f7fae, 0x9aa7b3, true),
        t::WINDOW_DARK => p.window(0x26323d, 0x8f8c86, true),
        t::WINDOW_WARM => p.window(0xf7c66a, 0x6b5b45, false),
        t::STEEL => {
            p.bevel(0x5e5a57);
            for (x, y) in [(2, 2), (13, 2), (2, 13), (13, 13)] {
                p.set(x, y, hex(0x8a857f), 1.0);
            }
        }
        t::RED_STEEL => {
            p.bevel(0xd3482f);
            for (x, y) in [(2, 2), (13, 2), (2, 13), (13, 13)] {
                p.set(x, y, hex(0xf07a5c), 1.0);
            }
        }
        t::COPPER => {
            p.noise(0x5fa68f, 0.15);
            p.specks(0x4a8a77, 0.15, 3);
            p.specks(0x7cc4ab, 0.08, 5);
        }
        t::GOLD_BLOCK => p.bevel(0xf2c938),
        t::IRON_BLOCK => p.bevel(0xd8d8d8),
        t::DIAMOND_BLOCK => p.bevel(0x62e3dc),
        t::LIMESTONE => {
            p.noise(0xdad0b2, 0.06);
            for x in 0..16 {
                p.set(x, 15, hex(0xc4b997), 1.0);
            }
            p.set(7, 15, hex(0xc4b997), 1.0);
        }
        t::SLATE => {
            for y in 0..16 {
                for x in 0..16 {
                    let row = y / 4;
                    let off = if row % 2 == 0 { 0 } else { 2 };
                    let edge = y % 4 == 3 || (x + off) % 4 == 0;
                    let k = 0.9 + p.r(x / 4, row, 2) * 0.2;
                    p.set(x, y, if edge { hex(0x37404c) } else { mul(hex(0x505b6a), k) }, 1.0);
                }
            }
        }
        _ if (t::NEON..t::NEON + 3).contains(&id) => {
            let c = [0xff3fa4, 0x34f0ff, 0xfff04a][(id - t::NEON) as usize];
            p.noise(c, 0.06);
            for i in 2..14 {
                p.set(i, 7, mix(hex(c), hex(0xffffff), 0.7), 1.0);
                p.set(i, 8, mix(hex(c), hex(0xffffff), 0.5), 1.0);
            }
        }
        t::TERRACOTTA => p.noise(0xa65b3b, 0.08),
        t::MARBLE => {
            p.noise(0xeeeeec, 0.04);
            let mut x = 2.0;
            for y in 0..16 {
                x += (p.r(0, y, 4) - 0.5) * 2.0;
                p.set(x as i32, y, hex(0xbdbdbd), 1.0);
            }
        }
        t::STICK => {
            p.clear();
            p.line(4.0, 13.0, 12.0, 3.0, hex(0x7a5a32));
            p.line(5.0, 13.0, 12.0, 4.0, hex(0x5e4426));
        }
        t::COAL => {
            p.clear();
            for y in 4..13 {
                for x in 4..13 {
                    if (x - 8) * (x - 8) + (y - 8) * (y - 8) < 18 {
                        p.set(x, y, mul(hex(0x2a2a2a), 0.8 + p.r(x, y, 1) * 0.5), 1.0);
                    }
                }
            }
        }
        t::IRON_INGOT | t::GOLD_INGOT => {
            p.clear();
            let c = if id == t::IRON_INGOT { 0xdcdcdc } else { 0xf5cf3a };
            for y in 6..11 {
                for x in (2 + (10 - y).max(0) / 2)..(14 - (10 - y).max(0) / 2) {
                    p.set(x, y, mul(hex(c), if y == 6 { 1.15 } else { 0.95 }), 1.0);
                }
            }
        }
        t::DIAMOND => {
            p.clear();
            for y in 3..13 {
                let w = if y < 6 { (y - 2) * 2 } else { 13 - y };
                for x in (8 - w)..(8 + w) {
                    p.set(x, y, mul(hex(0x62e9e2), 0.85 + ((x + y) % 3) as f32 * 0.1), 1.0);
                }
            }
        }
        _ if (t::PICK..t::PICK + 4).contains(&id) => p.sprite_kind([0xa07a45, 0x8a8a8a, 0xe5e5e5, 0x62e9e2][(id - t::PICK) as usize], 0),
        t::APPLE => {
            p.clear();
            for y in 4..14 {
                for x in 3..13 {
                    if (x - 8) * (x - 8) + (y - 9) * (y - 9) < 22 {
                        p.set(x, y, mul(hex(0xd8262b), 0.85 + p.r(x, y, 1) * 0.25), 1.0);
                    }
                }
            }
            p.line(8.0, 2.0, 8.0, 5.0, hex(0x5a3d1e));
            p.set(9, 3, hex(0x3f8a2b), 1.0);
            p.set(10, 3, hex(0x3f8a2b), 1.0);
        }
        t::BREAD => {
            p.clear();
            for y in 6..12 {
                for x in 2..14 {
                    p.set(x, y, mul(hex(0xc58a3c), if y == 6 { 1.15 } else { 0.95 }), 1.0);
                }
            }
            for x in [4, 7, 10] {
                p.set(x, 7, hex(0xe7c07a), 1.0);
            }
        }
        _ if (t::CRACK..t::CRACK + 10).contains(&id) => {
            p.clear();
            let stage = (id - t::CRACK) as i32 + 1;
            for i in 0..stage * 3 {
                let (mut x, mut y) = (7.0 + (p.r(i, 0, 1) - 0.5) * 6.0, 7.0 + (p.r(i, 1, 1) - 0.5) * 6.0);
                let a = p.r(i, 2, 1) * std::f32::consts::TAU;
                for _ in 0..(3 + stage / 2) {
                    p.set(x as i32, y as i32, hex(0x101010), 0.9);
                    x += a.cos() * 1.2 + (p.r(x as i32, y as i32, 5) - 0.5);
                    y += a.sin() * 1.2 + (p.r(y as i32, x as i32, 6) - 0.5);
                }
            }
        }
        t::WHITE => p.noise(0xffffff, 0.0),
        t::BLACK => p.noise(0x101010, 0.0),
        _ if (t::SKIN..t::SKIN + 4).contains(&id) => p.noise([0xf0c8a0, 0xd9a066, 0x9c6b3e, 0x5f3e24][(id - t::SKIN) as usize], 0.06),
        _ if (t::SHIRT..t::SHIRT + 8).contains(&id) => {
            p.noise([0x3fa7d6, 0xd64545, 0x47b35a, 0xe8b23a, 0x8a55c9, 0xeeeeee, 0x333333, 0xf07fb3][(id - t::SHIRT) as usize], 0.1);
        }
        t::PANTS => p.noise(0x2f4b7c, 0.12),
        t::HAIR => p.noise(0x4a3020, 0.15),
        _ if (t::FACE..t::FACE + 4).contains(&id) => {
            let skin = [0xf0c8a0, 0xd9a066, 0x9c6b3e, 0x5f3e24][(id - t::FACE) as usize];
            p.face(skin, [0x3060c0, 0x3a7a3a, 0x4a3020, 0x2a1a10][(id - t::FACE) as usize], Some(0x4a3020));
        }
        t::TORCH => {
            p.clear();
            for y in 6..16 {
                for x in 7..9 {
                    p.set(x, y, mul(hex(0x7a5a32), 0.9 + (x - 7) as f32 * 0.15), 1.0);
                }
            }
            for (x, y, c) in [(7, 5, 0xffd94a), (8, 5, 0xffd94a), (7, 6, 0xff9a2a), (8, 6, 0xffb43a), (7, 4, 0xfff3a0), (8, 4, 0xffe070)] {
                p.set(x, y, hex(c), 1.0);
            }
        }
        t::DOOR_TOP | t::DOOR_BOTTOM => {
            p.px = paint(t::PLANKS).px;
            p.frame(0x6b4f2d, 0);
            if id == t::DOOR_TOP {
                for y in 3..9 {
                    for x in [3, 4, 5, 10, 11, 12] {
                        p.set(x, y, hex(0xbfe3f5), 1.0);
                    }
                }
            } else {
                p.frame(0x8a6a42, 3);
                p.set(12, 2, hex(0x333333), 1.0);
                p.set(12, 3, hex(0x333333), 1.0);
            }
        }
        t::LADDER => {
            p.clear();
            for y in 0..16 {
                for x in [2, 3, 12, 13] {
                    p.set(x, y, mul(hex(0x8a6a3a), 0.9 + (y % 3) as f32 * 0.05), 1.0);
                }
            }
            for y in [2, 6, 10, 14] {
                for x in 2..14 {
                    p.set(x, y, hex(0x9c7a46), 1.0);
                }
            }
        }
        t::BED_TOP => {
            p.noise(0xb02e26, 0.08);
            for y in 0..5 {
                for x in 1..15 {
                    p.set(x, y, mul(hex(0xeeeeee), 0.95 + p.r(x, y, 2) * 0.05), 1.0);
                }
            }
        }
        t::BED_SIDE => {
            p.px = paint(t::PLANKS).px;
            for y in 0..6 {
                for x in 0..16 {
                    p.set(x, y, mul(hex(0xb02e26), 0.9 + p.r(x, y, 3) * 0.1), 1.0);
                }
            }
        }
        t::CHEST_TOP | t::CHEST_FRONT | t::CHEST_SIDE => {
            p.px = paint(t::PLANKS).px;
            p.frame(0x5a3d1e, 0);
            if id != t::CHEST_TOP {
                for x in 0..16 {
                    p.set(x, 5, hex(0x3d2a15), 1.0);
                }
            }
            if id == t::CHEST_FRONT {
                for (x, y) in [(7, 5), (8, 5), (7, 6), (8, 6), (7, 7), (8, 7)] {
                    p.set(x, y, hex(0xc9c9c9), 1.0);
                }
            }
        }
        t::FARMLAND => {
            p.noise(0x5a3c22, 0.2);
            for y in [3, 7, 11, 15] {
                for x in 0..16 {
                    p.set(x, y, hex(0x3d2814), 1.0);
                }
            }
        }
        _ if (t::WHEAT..t::WHEAT + 4).contains(&id) => {
            p.clear();
            let stage = (id - t::WHEAT) as f32;
            let top = 13.0 - stage * 3.3;
            let c = mix(hex(0x5f9e35), hex(0xd8b84a), stage / 3.0);
            for i in 0..5 {
                let x = 2.0 + i as f32 * 3.0;
                p.line(x, 15.0, x + (p.r(i, 0, 2) - 0.5) * 2.0, top, mul(c, 0.85 + p.r(i, 1, 2) * 0.3));
                if stage >= 3.0 {
                    p.line(x, top, x, top + 3.0, hex(0xe8c860));
                }
            }
        }
        t::SAPLING => {
            p.clear();
            p.line(8.0, 15.0, 8.0, 8.0, hex(0x6b5132));
            for y in 3..10 {
                for x in 4..13 {
                    if (x - 8) * (x - 8) + (y - 6) * (y - 6) < 12 && p.r(x, y, 4) < 0.85 {
                        p.set(x, y, mul(hex(0x3e8a2b), 0.8 + p.r(x, y, 5) * 0.4), 1.0);
                    }
                }
            }
        }
        t::COPPER_ORE => {
            p.noise(0x7d7d7d, 0.22);
            p.blobs(0xc9743d, 4, 11);
            p.blobs(0x5fa68f, 2, 13);
        }
        t::FURNACE_LIT => {
            p.px = paint(t::COBBLE).px;
            for y in 7..14 {
                for x in 4..12 {
                    let c = if y < 9 { 0xffd23a } else { 0xff7a1a };
                    p.set(x, y, mul(hex(c), 0.9 + p.r(x, y, 2) * 0.2), 1.0);
                }
            }
        }
        t::RAIL => {
            p.clear();
            for y in [1, 5, 9, 13] {
                for x in 1..15 {
                    p.set(x, y, hex(0x7a5a32), 1.0);
                    p.set(x, y + 1, hex(0x6b4f2d), 1.0);
                }
            }
            for y in 0..16 {
                for x in [3, 12] {
                    p.set(x, y, hex(0xb8b8b8), 1.0);
                }
            }
        }
        t::SEEDS => {
            p.clear();
            for i in 0..7 {
                let (x, y) = (4 + (p.r(i, 0, 3) * 8.0) as i32, 5 + (p.r(i, 1, 3) * 7.0) as i32);
                p.set(x, y, hex(0x6f8f2a), 1.0);
                p.set(x + 1, y, hex(0x55701e), 1.0);
            }
        }
        t::WHEAT_ITEM => {
            p.clear();
            for i in 0..4 {
                p.line(5.0 + i as f32, 14.0, 9.0 + i as f32 * 1.5, 2.0, hex(0xd8b84a));
            }
            p.line(5.0, 10.0, 12.0, 10.0, hex(0x8a6a2a));
        }
        t::COPPER_INGOT => {
            p.clear();
            for y in 6..11 {
                for x in (2 + (10 - y).max(0) / 2)..(14 - (10 - y).max(0) / 2) {
                    p.set(x, y, mul(hex(0xd47a42), if y == 6 { 1.15 } else { 0.95 }), 1.0);
                }
            }
        }
        _ if (t::AXE..t::AXE + 4).contains(&id) => p.sprite_kind([0xa07a45, 0x8a8a8a, 0xe5e5e5, 0x62e9e2][(id - t::AXE) as usize], 2),
        _ if (t::SHOVEL..t::SHOVEL + 4).contains(&id) => p.sprite_kind([0xa07a45, 0x8a8a8a, 0xe5e5e5, 0x62e9e2][(id - t::SHOVEL) as usize], 3),
        _ if (t::HOE..t::HOE + 4).contains(&id) => p.sprite_kind([0xa07a45, 0x8a8a8a, 0xe5e5e5, 0x62e9e2][(id - t::HOE) as usize], 4),
        _ if (t::ARMOR..t::ARMOR + 8).contains(&id) => {
            p.clear();
            let k = (id - t::ARMOR) as i32;
            let c = hex(if k % 2 == 0 { 0xdcdcdc } else { 0x62e9e2 });
            let piece = k / 2;
            for y in 0..16 {
                for x in 0..16 {
                    let on = match piece {
                        0 => (3..13).contains(&x) && (3..10).contains(&y) && !((5..11).contains(&x) && y >= 7),
                        1 => (2..14).contains(&x) && (2..14).contains(&y) && !((6..10).contains(&x) && y < 4) && !((x < 4 || x > 11) && y > 7),
                        2 => (3..13).contains(&x) && (2..14).contains(&y) && !((7..9).contains(&x) && y > 5),
                        _ => ((2..7).contains(&x) || (9..14).contains(&x)) && (8..14).contains(&y),
                    };
                    if on {
                        p.set(x, y, mul(c, 0.8 + (16 - y) as f32 * 0.015), 1.0);
                    }
                }
            }
        }
        t::BUCKET | t::WATER_BUCKET | t::LAVA_BUCKET => {
            p.clear();
            for y in 4..14 {
                let w = 6 - (y - 4) / 4;
                for x in (8 - w)..(8 + w) {
                    p.set(x, y, mul(hex(0xbcbcbc), 0.85 + (x as f32 - 4.0) * 0.02), 1.0);
                }
            }
            if id != t::BUCKET {
                let c = if id == t::WATER_BUCKET { 0x3466d4 } else { 0xff7a1a };
                for x in 3..13 {
                    p.set(x, 4, hex(c), 1.0);
                    p.set(x, 5, mul(hex(c), 0.85), 1.0);
                }
            }
            p.line(3.0, 4.0, 8.0, 1.0, hex(0x9a9a9a));
            p.line(8.0, 1.0, 13.0, 4.0, hex(0x9a9a9a));
        }
        t::REDSTONE_ORE => {
            p.noise(0x7d7d7d, 0.22);
            p.blobs(0xc81a12, 5, 21);
            p.specks(0xff4a3a, 0.04, 22);
        }
        t::WIRE | t::WIRE_ON => {
            p.clear();
            let c = if id == t::WIRE_ON { 0xff3020 } else { 0x6a0e0a };
            for i in 0..16 {
                for w in 7..9 {
                    let k = 0.85 + p.r(i, w, 3) * 0.3;
                    p.set(i, w, mul(hex(c), k), 1.0);
                    p.set(w, i, mul(hex(c), k), 1.0);
                }
            }
            for (x, y) in [(6, 6), (9, 6), (6, 9), (9, 9), (5, 8), (10, 7)] {
                p.set(x, y, hex(c), 1.0);
            }
        }
        t::LEVER => {
            p.clear();
            p.line(4.0, 14.0, 10.0, 4.0, hex(0x7a5a32));
            p.line(5.0, 14.0, 11.0, 4.0, hex(0x5e4529));
            for y in 12..16 {
                for x in 2..14 {
                    p.set(x, y, mul(hex(0x7d7d7d), 0.85 + p.r(x, y, 2) * 0.3), 1.0);
                }
            }
        }
        t::BUTTON => {
            p.clear();
            for y in 5..11 {
                for x in 4..12 {
                    p.set(x, y, mul(hex(0x8a8a8a), if y == 5 || x == 4 { 1.2 } else { 0.9 + p.r(x, y, 2) * 0.2 }), 1.0);
                }
            }
        }
        t::RTORCH | t::RTORCH_OFF => {
            p.clear();
            for y in 6..16 {
                for x in 7..9 {
                    p.set(x, y, mul(hex(0x7a5a32), 0.9 + (x - 7) as f32 * 0.15), 1.0);
                }
            }
            let (a, b) = if id == t::RTORCH { (0xff3a2a, 0xffb0a0) } else { (0x5a1410, 0x7a2018) };
            for (x, y, c) in [(7, 5, a), (8, 5, a), (7, 6, a), (8, 6, a), (7, 4, b), (8, 4, b)] {
                p.set(x, y, hex(c), 1.0);
            }
        }
        t::LAMP | t::LAMP_ON => {
            let (glass, glow) = if id == t::LAMP_ON { (0xf7c46a, 0xfff2c0) } else { (0x6a4a2a, 0x8a6a40) };
            p.noise(glass, 0.15);
            p.blobs(glow, 6, 4);
            p.frame(0x3d2a18, 0);
            for i in 0..16 {
                p.set(i, 8, hex(0x3d2a18), 1.0);
                p.set(8, i, hex(0x3d2a18), 1.0);
            }
        }
        t::REDSTONE_BLOCK => {
            p.noise(0xb3140c, 0.25);
            p.specks(0xff4030, 0.1, 3);
            p.frame(0x8a0e08, 0);
        }
        t::RAIL_EW => {
            let r = paint(t::RAIL);
            for y in 0..16 {
                for x in 0..16 {
                    p.px[(y * 16 + x) as usize] = r.px[(x * 16 + y) as usize];
                }
            }
        }
        _ if (t::RAIL_CURVE..t::RAIL_CURVE + 4).contains(&id) => {
            p.clear();
            // Ties and two curved rails joining the north edge with the east (then mirrored).
            let k = id - t::RAIL_CURVE;
            let (fx, fy) = (k == 1 || k == 3, k >= 2);
            let put = |p: &mut Painter, x: i32, y: i32, c: Rgb| {
                let (x, y) = (if fx { 15 - x } else { x }, if fy { 15 - y } else { y });
                p.set(x, y, c, 1.0);
            };
            for i in 0..5 {
                let a = (i as f32 + 0.5) / 5.0 * std::f32::consts::FRAC_PI_2;
                for r in 2..15 {
                    let (x, y) = (16.0 - (r as f32) * a.cos(), r as f32 * a.sin());
                    put(&mut p, x as i32, y as i32, hex(0x7a5a32));
                }
            }
            for r in [4.5f32, 12.5] {
                for j in 0..40 {
                    let a = j as f32 / 39.0 * std::f32::consts::FRAC_PI_2;
                    put(&mut p, (16.0 - r * a.cos()) as i32, (r * a.sin()) as i32, hex(0xb8b8b8));
                }
            }
        }
        _ if (t::POWERED_RAIL..t::POWERED_RAIL + 4).contains(&id) => {
            let k = id - t::POWERED_RAIL;
            let base = paint(if k % 2 == 1 { t::RAIL_EW } else { t::RAIL });
            p.px = base.px;
            let c = if k >= 2 { 0xff3020 } else { 0x6a0e0a };
            for i in 0..16 {
                if k % 2 == 1 {
                    p.set(i, 7, hex(c), 1.0);
                    p.set(i, 8, hex(0xe5c04a), 1.0);
                    for y in [3, 12] {
                        p.set(i, y, hex(0xe5c04a), 1.0);
                    }
                } else {
                    p.set(7, i, hex(c), 1.0);
                    p.set(8, i, hex(0xe5c04a), 1.0);
                    for x in [3, 12] {
                        p.set(x, i, hex(0xe5c04a), 1.0);
                    }
                }
            }
        }
        t::TRADE_TOP => {
            p.px = paint(t::PLANKS).px;
            for y in 3..13 {
                for x in 3..13 {
                    p.set(x, y, mul(hex(0x3f8a3a), 0.9 + p.r(x, y, 4) * 0.2), 1.0);
                }
            }
            for (x, y) in [(6, 6), (7, 5), (8, 5), (9, 6), (7, 9), (8, 9), (6, 8), (9, 8)] {
                p.set(x, y, hex(0x5ff07a), 1.0);
            }
        }
        t::TRADE_SIDE => {
            p.px = paint(t::PLANKS).px;
            p.frame(0x5a3d1e, 0);
            for x in 0..16 {
                p.set(x, 3, hex(0xb02e26), 1.0);
                p.set(x, 4, if x % 4 < 2 { hex(0xeeeeee) } else { hex(0xb02e26) }, 1.0);
            }
            for (x, y) in [(5, 9), (6, 8), (7, 8), (8, 9), (6, 10), (7, 10), (10, 9), (11, 8), (11, 10)] {
                p.set(x, y, hex(0x3fd060), 1.0);
            }
        }
        t::EMERALD_ORE => {
            p.noise(0x7d7d7d, 0.22);
            p.blobs(0x2fd060, 3, 31);
            p.specks(0x9cffb0, 0.02, 32);
        }
        t::EMERALD_BLOCK => {
            p.noise(0x2fb855, 0.18);
            p.bevel(0x2fb855);
            for i in 3..13 {
                p.set(i, i, hex(0x8affa8), 1.0);
            }
        }
        t::EMERALD => {
            p.clear();
            for y in 2..14 {
                let w = 6 - (y - 8i32).abs() / 2 - if y < 5 { 5 - y } else { 0 };
                for x in (8 - w.max(1))..(8 + w.max(1)) {
                    p.set(x, y, mul(hex(0x2fd060), 0.8 + (14 - y) as f32 * 0.03), 1.0);
                }
            }
            p.set(6, 5, hex(0xc0ffd0), 1.0);
            p.set(7, 4, hex(0xc0ffd0), 1.0);
        }
        t::EMBERSTONE => {
            p.noise(0x6e2a26, 0.3);
            p.specks(0x8a3a30, 0.15, 2);
            p.specks(0x4a1614, 0.12, 3);
        }
        t::EMBER_BRICK => {
            p.bricks(0x3a1a1c, 0x1e0c0e, 8, 4);
        }
        t::QUARTZ_ORE => {
            p.px = paint(t::EMBERSTONE).px;
            p.blobs(0xece6dc, 5, 7);
        }
        t::QUARTZ => {
            p.clear();
            for y in 4..13 {
                for x in 4..12 {
                    if (x + y) % 9 < 6 {
                        p.set(x, y, mul(hex(0xf2ece2), 0.85 + p.r(x, y, 2) * 0.2), 1.0);
                    }
                }
            }
        }
        t::ASH_SAND => {
            p.noise(0x54443a, 0.25);
            p.specks(0x3a2e26, 0.15, 3);
            p.specks(0x7a6a5a, 0.05, 4);
        }
        t::EMBER_PORTAL | t::SKY_PORTAL => {
            let (a, b) = if id == t::EMBER_PORTAL { (0x7a1fd0, 0xc070ff) } else { (0x3aa8e8, 0xe8f8ff) };
            for y in 0..16 {
                for x in 0..16 {
                    let w = ((x as f32 * 0.7 + (y as f32 * 0.9).sin() * 2.0).sin() * 0.5 + 0.5) * 0.6 + p.r(x, y, 2) * 0.4;
                    p.set(x, y, mix(hex(a), hex(b), w), 0.85);
                }
            }
        }
        t::CLOUDSTONE => {
            p.noise(0xd8dde6, 0.1);
            p.blobs(0xf4f6fa, 6, 3);
            p.specks(0xb8c0cc, 0.06, 4);
        }
        t::FLINT_STEEL => {
            p.clear();
            for y in 3..9 {
                for x in 3..9 {
                    if (x - 6) * (x - 6) + (y - 6) * (y - 6) < 10 {
                        p.set(x, y, mul(hex(0x3a3a3a), 0.8 + p.r(x, y, 2) * 0.4), 1.0);
                    }
                }
            }
            p.line(8.0, 13.0, 13.0, 8.0, hex(0xcfcfcf));
            p.line(8.0, 12.0, 12.0, 8.0, hex(0x9a9a9a));
            p.line(9.0, 13.0, 13.0, 9.0, hex(0x9a9a9a));
        }
        t::MINECART | t::TRAIN => {
            p.clear();
            let body = if id == t::TRAIN { 0x3a3a46 } else { 0x8a8a8a };
            for y in 5..12 {
                for x in 2..14 {
                    p.set(x, y, mul(hex(body), if y == 5 { 1.25 } else { 0.85 + p.r(x, y, 2) * 0.2 }), 1.0);
                }
            }
            if id == t::TRAIN {
                for y in 1..5 {
                    for x in 9..12 {
                        p.set(x, y, hex(0x222228), 1.0);
                    }
                }
                for x in 2..14 {
                    p.set(x, 8, hex(0xb02e26), 1.0);
                }
            }
            for x in [4, 11] {
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    p.set(x + dx, 12 + dy, hex(0x222222), 1.0);
                }
            }
        }
        t::BOAT => {
            p.clear();
            for y in 7..12 {
                let inset = (y - 7) as i32;
                for x in (1 + inset)..(15 - inset) {
                    p.set(x, y, mul(hex(0xa07a45), 0.85 + p.r(x, y, 2) * 0.2), 1.0);
                }
            }
            p.line(8.0, 2.0, 8.0, 7.0, hex(0x6b5132));
        }
        t::CAR => {
            p.clear();
            for y in 7..12 {
                for x in 1..15 {
                    p.set(x, y, mul(hex(0xc8281e), 0.9 + p.r(x, y, 2) * 0.15), 1.0);
                }
            }
            for y in 4..7 {
                for x in 4..12 {
                    p.set(x, y, if x == 4 || x == 11 || y == 4 { hex(0xa01e18) } else { hex(0xa8d8f0) }, 1.0);
                }
            }
            for x in [3, 11] {
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    p.set(x + dx, 12 + dy, hex(0x1a1a1a), 1.0);
                }
            }
        }
        t::TIRE => {
            p.noise(0x1e1e1e, 0.2);
            for i in 5..11 {
                p.set(i, 7, hex(0x9a9a9a), 1.0);
                p.set(i, 8, hex(0x9a9a9a), 1.0);
            }
        }
        _ if (t::CAR_PAINT..t::CAR_PAINT + 4).contains(&id) => {
            let c = [0xc8281e, 0xf2c21a, 0x2a5ad0, 0xeeeeee][(id - t::CAR_PAINT) as usize];
            p.noise(c, 0.06);
            for x in 0..16 {
                p.set(x, 0, mul(hex(c), 1.2), 1.0);
            }
        }
        t::HEADLIGHT => {
            p.noise(0xfff6c8, 0.05);
            p.frame(0x9a9a9a, 0);
        }
        t::RAIN => {
            p.clear();
            for y in 0..16 {
                for x in 7..9 {
                    p.set(x, y, hex(0x8fb4e8), 0.7);
                }
            }
        }
        t::SNOWFLAKE => p.noise(0xffffff, 0.02),
        t::SMOKE => p.noise(0x8a8a8a, 0.2),
        t::FLAME => {
            p.noise(0xffb43a, 0.2);
            p.specks(0xfff3a0, 0.2, 4);
        }
        _ => {
            // Missing tile: magenta checker, easy to spot.
            for y in 0..16 {
                for x in 0..16 {
                    p.set(x, y, hex(if (x / 4 + y / 4) % 2 == 0 { 0xff00ff } else { 0x111111 }), 1.0);
                }
            }
        }
    }
    p
}

pub fn build_atlas() -> Atlas {
    let mut rgba = vec![0u8; ATLAS * ATLAS * 4];
    for id in 0..t::COUNT as u8 {
        let p = paint(id);
        let (tx, ty) = (id as usize % PER_ROW, id as usize / PER_ROW);
        for y in 0..TILE {
            for x in 0..TILE {
                let c = p.px[y * 16 + x];
                let o = ((ty * TILE + y) * ATLAS + tx * TILE + x) * 4;
                rgba[o] = c[0].clamp(0.0, 255.0) as u8;
                rgba[o + 1] = c[1].clamp(0.0, 255.0) as u8;
                rgba[o + 2] = c[2].clamp(0.0, 255.0) as u8;
                rgba[o + 3] = (c[3] * 255.0).clamp(0.0, 255.0) as u8;
            }
        }
    }
    Atlas { rgba }
}

impl Atlas {
    pub fn texel(&self, tile: u8, x: usize, y: usize) -> [u8; 4] {
        let (tx, ty) = (tile as usize % PER_ROW, tile as usize / PER_ROW);
        let o = ((ty * TILE + y.min(15)) * ATLAS + tx * TILE + x.min(15)) * 4;
        [self.rgba[o], self.rgba[o + 1], self.rgba[o + 2], self.rgba[o + 3]]
    }

    /// A 32×32 inventory icon: an isometric cube for blocks, the sprite for items and plants.
    pub fn icon(&self, id: BlockId) -> Vec<u8> {
        const S: usize = 32;
        let mut out = vec![0u8; S * S * 4];
        let flat = block::is_flat(id);
        if flat {
            let tile = block::item_tile(id);
            for y in 0..S {
                for x in 0..S {
                    let c = self.texel(tile, x / 2, y / 2);
                    out[(y * S + x) * 4..(y * S + x) * 4 + 4].copy_from_slice(&c);
                }
            }
            return out;
        }
        let tex = block::def(id).tex;
        let mut plot = |px: f32, py: f32, c: [u8; 4], shade: f32| {
            let (x, y) = (px as i32, py as i32);
            if c[3] < 40 || !(0..S as i32).contains(&x) || !(0..S as i32).contains(&y) {
                return;
            }
            let o = (y as usize * S + x as usize) * 4;
            out[o] = (c[0] as f32 * shade) as u8;
            out[o + 1] = (c[1] as f32 * shade) as u8;
            out[o + 2] = (c[2] as f32 * shade) as u8;
            out[o + 3] = 255;
        };
        let n = 64;
        // (origin, s-axis, t-axis, tile, shade) for left, right, top faces, painted in that order.
        let faces: [([f32; 2], [f32; 2], [f32; 2], u8, f32); 3] = [
            ([2.0, 8.5], [14.0, 7.0], [0.0, 15.0], tex[1], 0.78),
            ([16.0, 15.5], [14.0, -7.0], [0.0, 15.0], tex[1], 0.6),
            ([16.0, 1.5], [14.0, 7.0], [-14.0, 7.0], tex[0], 1.0),
        ];
        for (o, sa, ta, tile, shade) in faces {
            for j in 0..n {
                for i in 0..n {
                    let (s, tt) = ((i as f32 + 0.5) / n as f32, (j as f32 + 0.5) / n as f32);
                    let c = self.texel(tile, (s * 16.0) as usize, (tt * 16.0) as usize);
                    plot(o[0] + sa[0] * s + ta[0] * tt, o[1] + sa[1] * s + ta[1] * tt, c, shade);
                }
            }
        }
        out
    }
}
