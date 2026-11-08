//! Hand-designed cities laid over the generated world: New York, Tokyo, Paris and London.
//!
//! Every column is decided from its own coordinates (street grid, lot, building spec), so the
//! server can generate any chunk alone. City layouts don't depend on the world seed: they are
//! the same in every world.

use crate::block::*;
use crate::chunk::SEA_LEVEL;
use crate::worldgen::{Tree, TreeKind, tree_blocks};
use crate::noise::{hash, hash2, hash3, unit};

/// City street level.
pub const GROUND: i32 = 64;
const G: i32 = GROUND;
/// Road width, then a 2-block sidewalk on each side of every lot.
const ROAD: i32 = 7;
const RING_RIVER: i32 = 24;
const BLEND: i32 = 60;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Style {
    NewYork,
    Tokyo,
    Paris,
    London,
}

#[derive(Debug)]
pub struct City {
    pub name: &'static str,
    pub blurb: &'static str,
    pub x: i32,
    pub z: i32,
    pub radius: i32,
    pub style: Style,
    /// Grid cell size: road + sidewalks + lot.
    pub bx: i32,
    pub bz: i32,
    /// Surrounded by a river with bridges (Manhattan).
    pub ring_river: bool,
    /// A spot worth arriving at, relative to the centre.
    pub arrive: (i32, i32),
}

pub static CITIES: [City; 4] = [
    City {
        name: "New York",
        blurb: "Skyscrapers, yellow cabs, Central Park, the Empire tower and Liberty island.",
        x: 1200,
        z: -400,
        radius: 330,
        style: Style::NewYork,
        bx: 44,
        bz: 30,
        ring_river: true,
        arrive: (3, -40),
    },
    City {
        name: "Tokyo",
        blurb: "Neon streets, the scramble crossing, Tokyo tower and a cherry blossom shrine.",
        x: -1400,
        z: 900,
        radius: 280,
        style: Style::Tokyo,
        bx: 30,
        bz: 26,
        ring_river: false,
        arrive: (3, 3),
    },
    City {
        name: "Paris",
        blurb: "Limestone boulevards, slate rooftops, the iron tower and the triumphal arch.",
        x: 400,
        z: 1700,
        radius: 260,
        style: Style::Paris,
        bx: 36,
        bz: 30,
        ring_river: false,
        arrive: (3, 60),
    },
    City {
        name: "London",
        blurb: "Brick terraces, the river, the clock tower, the big wheel and the glass shard.",
        x: -700,
        z: -1600,
        radius: 270,
        style: Style::London,
        bx: 34,
        bz: 28,
        ring_river: false,
        arrive: (30, 90),
    },
];

impl City {
    pub fn index(&'static self) -> usize {
        CITIES.iter().position(|c| std::ptr::eq(c, self)).unwrap_or(0)
    }
    fn salt(&self) -> u64 {
        hash(0xC1_7E5 ^ self.x as u64 ^ ((self.z as u64) << 20))
    }
    /// Where travelling here puts you (feet position).
    pub fn arrival(&self) -> (f32, f32, f32) {
        (self.x as f32 + self.arrive.0 as f32 + 0.5, G as f32 + 1.0, self.z as f32 + self.arrive.1 as f32 + 0.5)
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Zone {
    None,
    Core(&'static City),
    River(&'static City),
    /// Terrain easing from street level (t = 0) back to nature (t = 1).
    Blend(&'static City, f32),
}

pub fn zone(x: i32, z: i32) -> Zone {
    for c in CITIES.iter() {
        let d = (x - c.x).abs().max((z - c.z).abs());
        if d < c.radius {
            return Zone::Core(c);
        }
        let mut r = c.radius;
        if c.ring_river {
            if d < r + RING_RIVER {
                return Zone::River(c);
            }
            r += RING_RIVER;
        }
        if d < r + BLEND {
            let t = (d - r) as f32 / BLEND as f32;
            return Zone::Blend(c, t * t * (3.0 - 2.0 * t));
        }
    }
    Zone::None
}

type Put<'a> = &'a mut dyn FnMut(i32, BlockId);

/// Builds one city (or city river) column over the flattened terrain.
pub fn fill_column(_seed: u64, c: &'static City, x: i32, z: i32, put: Put<'_>) {
    let (lx, lz) = (x - c.x, z - c.z);
    let d = lx.abs().max(lz.abs());
    if d >= c.radius {
        river_ring(c, lx, lz, put);
        return;
    }
    if landmark(c, lx, lz, put) {
        return;
    }
    if c.style == Style::London && (THAMES.0..THAMES.1).contains(&lz) {
        river_crossing(c, lx, lz, true, THAMES, put);
        return;
    }
    let (u, v) = (lx.rem_euclid(c.bx), lz.rem_euclid(c.bz));
    let (gx, gz) = (lx.div_euclid(c.bx), lz.div_euclid(c.bz));
    if big_park(c, lx, lz) {
        park(c, lx, lz, u, v, put);
        return;
    }
    if u < ROAD || v < ROAD {
        road(c, lx, lz, u, v, gx, gz, put);
        return;
    }
    let side = ROAD + 2;
    if u < side || v < side || u >= c.bx - 2 || v >= c.bz - 2 {
        sidewalk(c, u, v, put);
        return;
    }
    if small_park(c, gx, gz) {
        park(c, lx, lz, u, v, put);
        return;
    }
    building(c, gx, gz, u - side, v - side, x, z, put);
}

// ---------------------------------------------------------------------------------------------
// Streets

fn road(c: &City, _lx: i32, _lz: i32, u: i32, v: i32, gx: i32, gz: i32, put: Put<'_>) {
    put(G, ASPHALT);
    let mid = ROAD / 2;
    let (av, st) = (u < ROAD, v < ROAD);
    if av && st {
        // The scramble crossing in the middle of Tokyo.
        if c.style == Style::Tokyo && gx == 0 && gz == 0 && (u == v || u + v == ROAD - 1) {
            put(G, ROAD_WHITE);
        }
        if c.style == Style::Paris && gx == 0 && gz == 0 {
            put(G, SIDEWALK);
        }
        return;
    }
    // `along` runs with the road; `across` across it.
    let (across, along, cell) = if av { (u, v, c.bz) } else { (v, u, c.bx) };
    let near_crossing = along < ROAD + 2 || along >= cell - 2;
    if near_crossing {
        if across % 2 == 0 {
            put(G, ROAD_WHITE);
        }
        return;
    }
    if across == mid && (along / 2) % 2 == 0 {
        put(G, if c.style == Style::NewYork || c.style == Style::Tokyo { ROAD_LINE } else { ROAD_WHITE });
    }
    // Parked cars: one per lane per block, sometimes.
    let seg = if av { hash2(c.salt() ^ 0xCA2, gx, gz) } else { hash2(c.salt() ^ 0xCA3, gx, gz) };
    for lane in 0..2 {
        let h = hash(seg ^ lane);
        if unit(h) > 0.65 {
            continue;
        }
        let lane_across = if lane == 0 { 1 } else { ROAD - 3 };
        if across != lane_across && across != lane_across + 1 {
            continue;
        }
        let span = cell - ROAD - 8;
        let start = ROAD + 3 + (h >> 20) as i32 % span.max(1);
        let k = along - start;
        if !(0..4).contains(&k) {
            continue;
        }
        let body = match c.style {
            Style::NewYork if (h >> 30) % 3 != 0 => WOOL_YELLOW,
            Style::London if (h >> 30) % 3 == 0 => WOOL_RED,
            _ => [WOOL_WHITE, WOOL_BLACK, WOOL_BLUE, WOOL_RED, CONCRETE_GRAY][(h >> 33) as usize % 5],
        };
        put(G + 1, if (k == 0 || k == 3) && (h >> 40) % 2 == 0 { WOOL_BLACK } else { body });
        if k == 1 || k == 2 {
            put(G + 2, GLASS);
            put(G + 3, body);
        }
    }
}

fn sidewalk(c: &City, u: i32, v: i32, put: Put<'_>) {
    put(G, SIDEWALK);
    let lamp = (u == ROAD && v % 12 == 6) || (v == ROAD && u % 12 == 6);
    if lamp {
        for y in 1..=4 {
            put(G + y, STEEL);
        }
        put(G + 5, LANTERN);
    }
    // Paris boulevards: low hedges along the curb.
    if c.style == Style::Paris && (u == ROAD || v == ROAD) && !lamp && (u + v) % 6 == 0 {
        put(G + 1, OAK_LEAVES);
    }
}

/// A road bridge, or open water, where a river crosses the street grid.
fn river_crossing(c: &City, lx: i32, lz: i32, along_z: bool, bank: (i32, i32), put: Put<'_>) {
    let (u, v) = (lx.rem_euclid(c.bx), lz.rem_euclid(c.bz));
    let (gx, gz) = (lx.div_euclid(c.bx), lz.div_euclid(c.bz));
    let edge = if along_z { lz == bank.0 || lz == bank.1 - 1 } else { false };
    for y in (SEA_LEVEL - 6)..=G {
        put(y, if y <= SEA_LEVEL { WATER } else { AIR });
    }
    put(SEA_LEVEL - 7, SAND);
    if edge {
        for y in (SEA_LEVEL - 6)..=G {
            put(y, STONE_BRICK);
        }
    }
    let (on_road, across, along) = if along_z { (u < ROAD && gx % 2 == 0, u, lz) } else { (v < ROAD && gz % 2 == 0, v, lx) };
    if on_road {
        put(G, if across == ROAD / 2 { ROAD_LINE } else { ASPHALT });
        put(G - 1, STONE_BRICK);
        if across == 0 || across == ROAD - 1 {
            put(G, STONE_BRICK);
            put(G + 1, STONE_BRICK);
            if along % 8 == 0 {
                put(G + 2, LANTERN);
            }
        }
        if along.rem_euclid(10) == 0 {
            for y in (SEA_LEVEL - 6)..G {
                put(y, STONE_BRICK);
            }
        }
    }
}

fn river_ring(c: &City, lx: i32, lz: i32, put: Put<'_>) {
    if c.style == Style::NewYork && liberty(c, lx, lz, put) {
        return;
    }
    // Bridges cross the ring on every other avenue (north/south) and every third street.
    let ns = lz.abs() > lx.abs();
    let (u, v) = (lx.rem_euclid(c.bx), lz.rem_euclid(c.bz));
    let (gx, gz) = (lx.div_euclid(c.bx), lz.div_euclid(c.bz));
    let bridge = if ns { u < ROAD && gx % 2 == 0 } else { v < ROAD && gz % 3 == 0 };
    if !bridge {
        return;
    }
    let (across, along) = if ns { (u, lz) } else { (v, lx) };
    put(G, if across == ROAD / 2 { ROAD_LINE } else { ASPHALT });
    put(G - 1, STEEL);
    if across == 0 || across == ROAD - 1 {
        put(G + 1, STEEL);
        if along % 6 == 0 {
            for y in 2..=6 {
                put(G + y, STEEL);
            }
            put(G + 7, LANTERN);
        }
    }
    if along.rem_euclid(8) == 0 && (across == 1 || across == ROAD - 2) {
        for y in (SEA_LEVEL - 6)..(G - 1) {
            put(y, STONE_BRICK);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Parks

fn big_park(c: &City, lx: i32, lz: i32) -> bool {
    let inside = |x0: i32, z0: i32, x1: i32, z1: i32| (x0..x1).contains(&lx) && (z0..z1).contains(&lz);
    match c.style {
        Style::NewYork => inside(51, -263, 132, -90),
        Style::Tokyo => inside(67, -71, 120, -26),
        Style::Paris => inside(-34, 104, 38, 178),
        Style::London => inside(-197, 35, -109, 112) && !(THAMES.0..THAMES.1).contains(&lz),
    }
}

fn small_park(c: &City, gx: i32, gz: i32) -> bool {
    unit(hash2(c.salt() ^ 0xFA2C, gx, gz)) < 0.07
}

fn pond(c: &City, lx: i32, lz: i32) -> bool {
    let ell = |cx: f32, cz: f32, rx: f32, rz: f32| {
        let (dx, dz) = ((lx as f32 - cx) / rx, (lz as f32 - cz) / rz);
        dx * dx + dz * dz < 1.0
    };
    match c.style {
        Style::NewYork => ell(91.0, -200.0, 18.0, 30.0),
        Style::Tokyo => ell(80.0, -40.0, 6.0, 5.0),
        Style::London => ell(-150.0, 80.0, 30.0, 8.0),
        Style::Paris => false,
    }
}

fn is_path(c: &City, lx: i32, lz: i32) -> bool {
    let (u, v) = (lx.rem_euclid(c.bx), lz.rem_euclid(c.bz));
    (2..5).contains(&u) || (2..5).contains(&v)
}

fn park_ground(c: &City, lx: i32, lz: i32) -> bool {
    let d = lx.abs().max(lz.abs());
    if d >= c.radius - 2 {
        return false;
    }
    let (u, v) = (lx.rem_euclid(c.bx), lz.rem_euclid(c.bz));
    let side = ROAD + 2;
    let in_lot = u >= side && v >= side && u < c.bx - 2 && v < c.bz - 2;
    let park = big_park(c, lx, lz) || (in_lot && small_park(c, lx.div_euclid(c.bx), lz.div_euclid(c.bz)));
    park && !pond(c, lx, lz) && !is_path(c, lx, lz) && !landmark_area(c, lx, lz)
}

const PARK_CELL: i32 = 7;

fn park_tree(c: &City, gx: i32, gz: i32) -> Option<Tree> {
    let h = hash2(c.salt() ^ 0x7A2C, gx, gz);
    if unit(h) > 0.55 {
        return None;
    }
    let x = gx * PARK_CELL + 1 + (h >> 24) as i32 % (PARK_CELL - 2);
    let z = gz * PARK_CELL + 1 + (h >> 32) as i32 % (PARK_CELL - 2);
    let clear = [(-3, -3), (3, -3), (-3, 3), (3, 3), (0, 0)].iter().all(|&(dx, dz)| park_ground(c, x + dx, z + dz));
    if !clear {
        return None;
    }
    let kind = match c.style {
        Style::Tokyo => TreeKind::Cherry,
        Style::London if (h >> 40) % 3 == 0 => TreeKind::Birch,
        _ if (h >> 40) % 5 == 0 => TreeKind::Birch,
        _ => TreeKind::Oak,
    };
    Some(Tree { x, y: G, z, kind, height: 4 + (h >> 44) as i32 % 3 })
}

fn park(c: &City, lx: i32, lz: i32, _u: i32, _v: i32, put: Put<'_>) {
    if pond(c, lx, lz) {
        put(G, WATER);
        put(G - 1, WATER);
        put(G - 2, CLAY);
        return;
    }
    if is_path(c, lx, lz) {
        put(G, GRAVEL);
    } else {
        put(G, GRASS);
        let r = unit(hash2(c.salt() ^ 0x9A55, lx, lz));
        if r < 0.1 {
            put(G + 1, TALL_GRASS);
        } else if r < 0.13 {
            put(G + 1, if r < 0.115 { FLOWER_RED } else { FLOWER_YELLOW });
        }
    }
    // Trees from nearby cells whose canopy reaches this column.
    let mut col = [AIR; 16];
    let (gx, gz) = (lx.div_euclid(PARK_CELL), lz.div_euclid(PARK_CELL));
    for tz in gz - 1..=gz + 1 {
        for tx in gx - 1..=gx + 1 {
            let Some(t) = park_tree(c, tx, tz) else { continue };
            tree_blocks(c.salt(), &t, &mut |x, y, z, b, force| {
                let i = y - G;
                if x == lx && z == lz && (1..16).contains(&i) && (force || col[i as usize] == AIR) {
                    col[i as usize] = b;
                }
            });
        }
    }
    for (i, &b) in col.iter().enumerate().skip(1) {
        if b != AIR {
            put(G + i as i32, b);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Buildings

#[derive(Clone, Copy)]
struct Spec {
    /// Height above the street, a multiple of 4.
    h: i32,
    wall: BlockId,
    glass: BlockId,
    pillar: BlockId,
    floor: BlockId,
    roof: BlockId,
    /// 0: curtain wall, 1: every other column glass, 2: two of three.
    pattern: u8,
    /// Wedding-cake setbacks (inset, from height).
    setbacks: [(i32, i32); 4],
    spire: i32,
    neon: u8,
    mansard: bool,
    water_tower: bool,
}

impl Spec {
    fn plain(h: i32, wall: BlockId, glass: BlockId) -> Spec {
        Spec {
            h,
            wall,
            glass,
            pillar: wall,
            floor: CONCRETE_GRAY,
            roof: CONCRETE_GRAY,
            pattern: 2,
            setbacks: [(0, i32::MAX); 4],
            spire: 0,
            neon: 0,
            mansard: false,
            water_tower: false,
        }
    }
}

fn round4(h: f32) -> i32 {
    ((h / 4.0).round() as i32).max(2) * 4
}

fn spec_for(c: &City, gx: i32, gz: i32, part: u64) -> Spec {
    let h = hash(hash2(c.salt() ^ 0xB1D6, gx, gz) ^ part.wrapping_mul(0x9E37));
    let r = unit(h);
    let r2 = unit(h >> 7);
    // Taller toward the middle of town.
    let (cx, cz) = (gx * c.bx + c.bx / 2, gz * c.bz + c.bz / 2);
    let f = (1.0 - cx.abs().max(cz.abs()) as f32 / c.radius as f32).clamp(0.0, 1.0);
    match c.style {
        Style::NewYork => {
            let height = round4(14.0 + f.powf(1.2) * 150.0 * (0.35 + 0.65 * r));
            let mut s = match (h >> 12) % 5 {
                0 => Spec { pattern: 0, pillar: STEEL, ..Spec::plain(height, CONCRETE_GRAY, WINDOW_BLUE) },
                1 => Spec { pattern: 2, ..Spec::plain(height, BRICK, WINDOW_DARK) },
                2 => Spec { pattern: 1, ..Spec::plain(height, LIMESTONE, WINDOW_DARK) },
                3 => Spec { pattern: 0, pillar: CONCRETE_DARK, ..Spec::plain(height, CONCRETE_DARK, WINDOW_DARK) },
                _ => Spec { pattern: 2, ..Spec::plain(height, STONE_BRICK, WINDOW_BLUE) },
            };
            if height > 60 {
                s.setbacks = [(2, height * 55 / 100), (4, height * 75 / 100), (6, height * 90 / 100), (0, i32::MAX)];
            }
            if height > 100 && r2 < 0.4 {
                s.spire = height / 6;
            }
            s.water_tower = height < 50 && r2 > 0.6;
            if gx == 0 && (gz == -1 || gz == -2) {
                s.neon = 3; // Times Square
            }
            s
        }
        Style::Tokyo => {
            let height = round4(10.0 + f.powf(1.1) * 100.0 * (0.25 + 0.75 * r));
            let mut s = match (h >> 12) % 4 {
                0 => Spec::plain(height, CONCRETE_WHITE, WINDOW_DARK),
                1 => Spec { pattern: 0, pillar: STEEL, ..Spec::plain(height, CONCRETE_GRAY, WINDOW_BLUE) },
                2 => Spec { pattern: 1, ..Spec::plain(height, TERRACOTTA, WINDOW_DARK) },
                _ => Spec::plain(height, CONCRETE_GRAY, WINDOW_DARK),
            };
            s.neon = if r2 < 0.6 { 1 + (h >> 20) as u8 % 2 } else { 0 };
            s
        }
        Style::Paris => {
            let height = if r < 0.5 { 20 } else { 24 };
            Spec { pattern: 1, mansard: true, roof: SLATE, floor: PLANKS, ..Spec::plain(height, LIMESTONE, WINDOW_DARK) }
        }
        Style::London => {
            if r < 0.06 && f > 0.3 {
                let height = round4(50.0 + 70.0 * r2);
                return Spec { pattern: 0, pillar: STEEL, ..Spec::plain(height, CONCRETE_GRAY, WINDOW_BLUE) };
            }
            let height = 12 + 4 * ((h >> 14) % 3) as i32;
            let wall = if (h >> 18) % 4 == 0 { LIMESTONE } else { BRICK };
            Spec { pattern: 1, roof: SLATE, floor: PLANKS, ..Spec::plain(height, wall, WINDOW_DARK) }
        }
    }
}

fn building(c: &City, gx: i32, gz: i32, iu: i32, iv: i32, x: i32, z: i32, put: Put<'_>) {
    let w = c.bx - ROAD - 4;
    let d = c.bz - ROAD - 4;
    let h = hash2(c.salt() ^ 0x1075, gx, gz);
    let parts = match c.style {
        Style::NewYork => 1 + (h % 2) as i32,
        Style::Tokyo => 1 + (h % 3) as i32,
        Style::Paris | Style::London => 2 + (h % 3) as i32,
    };
    let p = iu * parts / w;
    let start = (0..w).find(|&i| i * parts / w == p).unwrap_or(0);
    let end = (0..w).rev().find(|&i| i * parts / w == p).map_or(w, |e| e + 1);
    let spec = spec_for(c, gx, gz, p as u64);
    bcol(&spec, iu - start, iv, end - start, d, x, z, put);
}

fn neon_block(x: i32, y: i32, z: i32) -> BlockId {
    [NEON_PINK, NEON_CYAN, NEON_YELLOW][(hash3(0x2E0, x / 3, y / 6, z / 3) % 3) as usize]
}

/// One column of a building with footprint `fw`×`fd`, at (fu, fv) inside it.
fn bcol(s: &Spec, fu: i32, fv: i32, fw: i32, fd: i32, x: i32, z: i32, put: Put<'_>) {
    put(G, s.floor);
    let top = G + s.h;
    let max_inset = (fw.min(fd) - 3) / 2;
    for y in (G + 1)..=(top + 1) {
        let rel = y - G - 1;
        let mut k = s.setbacks.iter().filter(|(_, from)| rel >= *from).map(|(i, _)| *i).max().unwrap_or(0).min(max_inset.max(0));
        let mansard = s.mansard && y > top - 8;
        if mansard {
            k = k.max(1);
        }
        let inside = fu >= k && fu < fw - k && fv >= k && fv < fd - k;
        if !inside {
            if y > top || k > 0 {
                // Above a setback the column is open sky; nothing higher can be inside.
                continue;
            }
            continue;
        }
        let xe = fu == k || fu == fw - 1 - k;
        let ze = fv == k || fv == fd - 1 - k;
        let edge = xe || ze;
        if y == top + 1 {
            if edge && !s.mansard {
                put(y, s.wall);
            }
            continue;
        }
        // A floor whose level is a setback level gets a roof where the tower steps in.
        let k_next = s.setbacks.iter().filter(|(_, from)| rel + 1 >= *from).map(|(i, _)| *i).max().unwrap_or(0).min(max_inset.max(0));
        let stepped = !(fu >= k_next && fu < fw - k_next && fv >= k_next && fv < fd - k_next);
        if y == top || (stepped && y < top) {
            put(y, s.roof);
            if stepped {
                continue;
            }
            continue;
        }
        let fy = rel % 4;
        if edge {
            let corner = xe && ze;
            let along = if xe { fv } else { fu };
            let mut b = if corner {
                s.pillar
            } else if rel < 4 {
                // Street level: a door facing the road, shop windows elsewhere.
                let mid = if ze { fw / 2 } else { fd / 2 };
                if fv == k && ze && (along == mid || along == mid - 1) && fy >= 1 && fy <= 2 {
                    AIR
                } else if fy == 1 || fy == 2 {
                    GLASS
                } else {
                    s.wall
                }
            } else if mansard {
                if fy == 2 && along % 3 == 1 { WINDOW_DARK } else { SLATE }
            } else if fy == 0 {
                s.wall
            } else {
                match s.pattern {
                    0 => s.glass,
                    1 => {
                        if along % 2 == 1 { s.glass } else { s.wall }
                    }
                    _ => {
                        if along % 3 != 0 { s.glass } else { s.wall }
                    }
                }
            };
            if (b == WINDOW_DARK || b == WINDOW_BLUE) && unit(hash3(0x11E, x, y, z)) < 0.14 {
                b = WINDOW_WARM;
            }
            if s.neon > 0 && !corner && rel >= 4 && y < top - 1 {
                let stripe = match s.neon {
                    3 => along % 3 == 1 && rel < 40,
                    2 => along % 5 == 2 && rel < 28,
                    _ => along % 7 == 3 && rel < 20,
                };
                let band = s.neon >= 2 && rel % 12 == 6;
                if stripe || band {
                    b = neon_block(x, y, z);
                }
            }
            put(y, b);
        } else if fy == 0 {
            let light = fu % 5 == 2 && fv % 5 == 2;
            put(y, if light { GLOWSTONE } else { s.floor });
        }
    }
    let (cx, cz) = (fw / 2, fd / 2);
    if s.spire > 0 && fu == cx && fv == cz {
        for y in (top + 1)..=(top + s.spire).min(250) {
            put(y, STEEL);
        }
        put((top + s.spire + 1).min(251), LANTERN);
    }
    if s.water_tower {
        let (dx, dz) = (fu - cx, fv - cz);
        if dx.abs() <= 1 && dz.abs() <= 1 {
            let corner = dx.abs() == 1 && dz.abs() == 1;
            if corner {
                put(top + 1, STEEL);
                put(top + 2, STEEL);
            }
            for y in (top + 3)..=(top + 6) {
                put(y, PLANKS);
            }
            if !corner {
                put(top + 7, SPRUCE_LOG);
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Landmarks

const THAMES: (i32, i32) = (44, 68);

fn landmark_area(c: &City, lx: i32, lz: i32) -> bool {
    let r = |x0: i32, z0: i32, x1: i32, z1: i32| (x0..x1).contains(&lx) && (z0..z1).contains(&lz);
    match c.style {
        Style::NewYork => r(-35, 39, -2, 58),
        Style::Tokyo => r(-93, 47, -57, 83),
        Style::Paris => r(-34, 106, 36, 176) || r(-13, -13, 20, 20),
        Style::London => r(10, 70, 90, 86) || r(40, 26, 100, 44) || r(-96, -96, -64, -64),
    }
}

fn landmark(c: &City, lx: i32, lz: i32, put: Put<'_>) -> bool {
    if !landmark_area(c, lx, lz) {
        return false;
    }
    match c.style {
        Style::NewYork => empire(lx + 35, lz - 39, lx, lz, put),
        Style::Tokyo => tokyo_tower(lx + 75, lz - 65, put),
        Style::Paris => {
            if (-13..20).contains(&lx) && (-13..20).contains(&lz) {
                arch(lx - 3, lz - 3, put)
            } else {
                eiffel(lx - 1, lz - 141, put)
            }
        }
        Style::London => {
            if (40..100).contains(&lx) && (26..44).contains(&lz) {
                big_wheel(lx, lz, put)
            } else if lz < 0 {
                shard(lx + 80, lz + 80, put)
            } else {
                parliament(lx, lz, put)
            }
        }
    }
    true
}

/// The stepped limestone tower in midtown Manhattan.
fn empire(fu: i32, fv: i32, x: i32, z: i32, put: Put<'_>) {
    let s = Spec {
        pattern: 1,
        setbacks: [(3, 20), (5, 72), (7, 96), (8, 108)],
        roof: LIMESTONE,
        ..Spec::plain(116, LIMESTONE, WINDOW_DARK)
    };
    bcol(&s, fu, fv, 33, 19, x, z, put);
    let (dx, dz) = (fu - 16, fv - 9);
    if dx.abs() <= 1 && dz.abs() <= 1 {
        for y in (G + 117)..(G + 134) {
            put(y, if (y - G) % 4 == 0 { LIMESTONE } else { WINDOW_DARK });
        }
    }
    if dx == 0 && dz == 0 {
        for y in (G + 134)..(G + 160) {
            put(y, STEEL);
        }
        put(G + 160, LANTERN);
    }
}

/// The copper statue on her island in the south river.
fn liberty(c: &City, lx: i32, lz: i32, put: Put<'_>) -> bool {
    let (dx, dz) = (lx, lz - (c.radius + 12));
    let d2 = dx * dx + dz * dz;
    if d2 > 100 {
        return false;
    }
    for y in (SEA_LEVEL - 6)..G {
        put(y, STONE);
    }
    put(G, if d2 > 64 { SAND } else { GRASS });
    if dx.abs() <= 4 && dz.abs() <= 4 {
        for y in (G + 1)..=(G + 14) {
            put(y, if y == G + 14 || dx.abs() == 4 || dz.abs() == 4 { STONE_BRICK } else { STONE });
        }
        put(G, STONE_BRICK);
    }
    let fd = ((dx * dx + dz * dz) as f32).sqrt();
    for y in (G + 15)..=(G + 38) {
        let r = 3.2 - (y - G - 15) as f32 * 0.06;
        if fd <= r {
            put(y, COPPER);
        }
    }
    for y in (G + 39)..=(G + 42) {
        let dy = (y - (G + 40)) as f32;
        if fd * fd + dy * dy <= 4.5 {
            put(y, COPPER);
        }
    }
    if (dx.abs() == 2 && dz == 0) || (dz.abs() == 2 && dx == 0) {
        put(G + 43, COPPER);
    }
    if dx == 3 && dz == 0 {
        for y in (G + 32)..=(G + 47) {
            put(y, COPPER);
        }
        put(G + 48, GOLD_BLOCK);
        put(G + 49, GLOWSTONE);
    }
    if dx == -3 && dz == 1 {
        for y in (G + 29)..=(G + 33) {
            put(y, COPPER);
        }
    }
    true
}

/// Red-and-white lattice tower, with two observation decks.
fn tokyo_tower(dx: i32, dz: i32, put: Put<'_>) {
    put(G, CONCRETE_WHITE);
    let (ax, az) = (dx.abs() as f32, dz.abs() as f32);
    let height = 150;
    for rel in 0..height {
        let y = G + 1 + rel;
        let t = rel as f32 / height as f32;
        let w = 1.6 + 15.0 * (1.0 - t).powf(1.9);
        let color = if (rel / 10) % 2 == 0 { RED_STEEL } else { CONCRETE_WHITE };
        let deck = |at: i32, extra: f32| -> Option<BlockId> {
            let r = rel - at;
            if !(0..5).contains(&r) {
                return None;
            }
            let ww = w + extra;
            if ax > ww || az > ww {
                return None;
            }
            let rim = ax > ww - 1.0 || az > ww - 1.0;
            Some(match r {
                0 | 4 => color,
                _ if rim => GLASS,
                _ => AIR,
            })
        };
        if let Some(b) = deck(44, 2.0).or_else(|| deck(100, 1.5)) {
            if b != AIR {
                put(y, b);
            }
            continue;
        }
        let near = |a: f32| (a - w).abs() < 0.8;
        let face_x = near(ax) && az <= w + 0.5;
        let face_z = near(az) && ax <= w + 0.5;
        let corner = near(ax) && near(az);
        if rel < 24 {
            // The four legs stand apart at the bottom.
            if ax > w - 3.0 && az > w - 3.0 && ax <= w + 0.5 && az <= w + 0.5 {
                put(y, color);
            }
            continue;
        }
        if corner {
            put(y, color);
        } else if face_x || face_z {
            let along = if face_x { dz } else { dx };
            if (rel + along).rem_euclid(6) == 0 || (rel - along).rem_euclid(6) == 0 || rel % 12 == 0 {
                put(y, color);
            }
        }
    }
    if dx == 0 && dz == 0 {
        for y in (G + 1 + height)..(G + height + 18) {
            put(y, if (y / 4) % 2 == 0 { RED_STEEL } else { CONCRETE_WHITE });
        }
        put(G + height + 18, LANTERN);
    }
}

/// The iron lattice tower on its lawn.
fn eiffel(dx: i32, dz: i32, put: Put<'_>) {
    let (ax, az) = (dx.abs() as f32, dz.abs() as f32);
    let path = dx.abs() <= 2 || dz.abs() <= 2 || (dx.abs() - dz.abs()).abs() <= 1;
    put(G, if path { GRAVEL } else { GRASS });
    let height = 178;
    for rel in 0..height {
        let y = G + 1 + rel;
        let t = rel as f32 / height as f32;
        let w = 1.2 + 29.0 * (1.0 - t).powf(2.6);
        let near = |a: f32| (a - w).abs() < 0.8;
        let platform = |at: i32| rel == at || (rel == at + 1 && (ax > w || az > w));
        if (platform(52) || platform(110)) && ax <= w + 1.5 && az <= w + 1.5 {
            put(y, STEEL);
            continue;
        }
        if rel < 52 {
            // Four splayed legs joined by arches.
            let leg = 6.0 * (1.0 - t * 1.2).max(0.45);
            let in_leg = ax > w - leg && az > w - leg && ax <= w + 0.5 && az <= w + 0.5;
            if in_leg {
                let shell = near(ax) || near(az) || (ax - (w - leg)).abs() < 0.8 || (az - (w - leg)).abs() < 0.8;
                if shell && ((rel + dx + dz).rem_euclid(3) != 0 || near(ax) && near(az)) {
                    put(y, STEEL);
                }
                continue;
            }
            let span = (w - leg).max(1.0);
            let arch_y = |a: f32| 46.0 - (a / span).powi(2) * 22.0;
            if (near(az) && ax < span && (rel as f32 - arch_y(ax)).abs() < 0.9)
                || (near(ax) && az < span && (rel as f32 - arch_y(az)).abs() < 0.9)
            {
                put(y, STEEL);
            }
            continue;
        }
        let corner = near(ax) && near(az) && ax <= w + 0.5;
        let face_x = near(ax) && az <= w;
        let face_z = near(az) && ax <= w;
        if corner {
            put(y, STEEL);
        } else if face_x || face_z {
            let along = if face_x { dz } else { dx };
            if (rel + along).rem_euclid(5) == 0 || (rel - along).rem_euclid(5) == 0 {
                put(y, STEEL);
            }
        }
    }
    if dx == 0 && dz == 0 {
        for y in (G + 1 + height)..(G + height + 8) {
            put(y, STEEL);
        }
        put(G + height + 8, LANTERN);
    }
}

/// The triumphal arch in the middle of the roundabout.
fn arch(dx: i32, dz: i32, put: Put<'_>) {
    let ring = dx * dx + dz * dz;
    put(G, if ring < 13 * 13 && ring > 9 * 9 { ASPHALT } else { SIDEWALK });
    if dx.abs() > 7 || dz.abs() > 4 {
        return;
    }
    for rel in 0..20 {
        let y = G + 1 + rel;
        let tunnel_x = dx.abs() <= 2 && (rel as f32) < 10.0 + (4.0 - (dx * dx) as f32).max(0.0).sqrt() * 1.5;
        let tunnel_z = dz.abs() <= 1 && rel < 7 && dx.abs() < 7;
        if tunnel_x || tunnel_z {
            continue;
        }
        let b = if rel == 15 || rel == 19 { MARBLE } else { LIMESTONE };
        put(y, b);
    }
}

/// The clock tower and the long parliament building beside the Thames.
fn parliament(lx: i32, lz: i32, put: Put<'_>) {
    put(G, SIDEWALK);
    // Clock tower: 8×8 at (10..18, 70..78).
    if (10..18).contains(&lx) && (70..78).contains(&lz) {
        let (fu, fv) = (lx - 10, lz - 70);
        let edge = fu == 0 || fu == 7 || fv == 0 || fv == 7;
        let corner = (fu == 0 || fu == 7) && (fv == 0 || fv == 7);
        for rel in 0..60 {
            let y = G + 1 + rel;
            let b = if !edge {
                if rel % 6 == 0 { PLANKS } else { continue }
            } else if corner {
                SANDSTONE
            } else if rel >= 46 && rel < 54 {
                // Clock faces: a gold rim around a white dial with two hands.
                let along = if fu == 0 || fu == 7 { fv } else { fu };
                let (a, r) = (along - 1, rel - 46);
                let rim = a == 0 || a == 5 || r == 0 || r == 7;
                if rim {
                    GOLD_BLOCK
                } else if (a == 2 && (2..=4).contains(&r)) || (r == 4 && (2..=4).contains(&a)) {
                    WOOL_BLACK
                } else {
                    CONCRETE_WHITE
                }
            } else if rel % 6 == 0 {
                SANDSTONE
            } else if (fu + fv) % 2 == 1 {
                WINDOW_DARK
            } else {
                SANDSTONE
            };
            put(y, b);
        }
        // Belfry, then a slate spire with a gold tip.
        for rel in 60..66 {
            let open = edge && !corner && (1..5).contains(&(rel - 60)) && (fu == 3 || fu == 4 || fv == 3 || fv == 4);
            if !open && (edge || rel == 60) {
                put(G + 1 + rel, SANDSTONE);
            }
        }
        for i in 0..4 {
            let lo = i;
            let hi = 7 - i;
            if (lo..=hi).contains(&fu) && (lo..=hi).contains(&fv) {
                for k in 0..4 {
                    put(G + 67 + i * 4 + k, SLATE);
                }
            }
        }
        if fu == 3 || fu == 4 {
            if fv == 3 || fv == 4 {
                put(G + 83, GOLD_BLOCK);
                put(G + 84, GOLD_BLOCK);
            }
        }
        return;
    }
    // The parliament wing: 5 floors with turrets every 12 blocks.
    if (18..90).contains(&lx) && (72..84).contains(&lz) {
        let s = Spec { pattern: 1, roof: SLATE, floor: PLANKS, ..Spec::plain(20, SANDSTONE, WINDOW_DARK) };
        bcol(&s, lx - 18, lz - 72, 72, 12, lx, lz, put);
        if (lx - 18) % 12 == 0 && (lz == 72 || lz == 83) {
            for y in (G + 21)..(G + 28) {
                put(y, SANDSTONE);
            }
            put(G + 28, GOLD_BLOCK);
        }
    }
}

/// The observation wheel on the north bank.
fn big_wheel(lx: i32, lz: i32, put: Put<'_>) {
    put(G, SIDEWALK);
    let (cx, cy, r) = (70, G + 34, 30.0f32);
    let plane = 36;
    if lz == plane {
        let dx = (lx - cx) as f32;
        for y in (G + 1)..(G + 70) {
            let dy = (y - cy) as f32;
            let d = (dx * dx + dy * dy).sqrt();
            let ring = (d - r).abs() < 0.75;
            let ang = dy.atan2(dx);
            let spoke = d < r && (0..12).any(|i| {
                let a = i as f32 * std::f32::consts::TAU / 12.0;
                (dx * a.sin() - dy * a.cos()).abs() < 0.55 && (dx * a.cos() + dy * a.sin()) > 0.0
            });
            let pod = (0..24).any(|i| {
                let a = i as f32 * std::f32::consts::TAU / 24.0;
                let (px, py) = (a.cos() * (r + 1.6), a.sin() * (r + 1.6));
                (dx - px).powi(2) + (dy - py).powi(2) < 1.6
            });
            let _ = ang;
            if ring || spoke || d < 1.5 {
                put(y, STEEL);
            } else if pod {
                put(y, GLASS);
            }
        }
    }
    // A-frame legs down to the plaza.
    if lz == plane - 2 || lz == plane + 2 {
        let dx = (lx - cx).abs();
        if dx <= 18 {
            let y_leg = cy - (dx * 34 / 18);
            for y in (y_leg - 1)..=(y_leg + 1) {
                if y > G {
                    put(y, STEEL);
                }
            }
        }
    }
    if (lz == plane - 1 || lz == plane + 1) && lx == cx {
        put(cy, STEEL);
    }
}

/// A glass pyramid skyscraper south of the river.
fn shard(dx: i32, dz: i32, put: Put<'_>) {
    put(G, SIDEWALK);
    let (ax, az) = (dx.abs(), dz.abs());
    let height = 150;
    for rel in 0..height {
        let w = 13.0 * (1.0 - rel as f32 / height as f32);
        let m = ax.max(az) as f32;
        if m > w {
            break;
        }
        let y = G + 1 + rel;
        if m > w - 1.0 {
            let b = if rel < 4 && (ax <= 1 || az <= 1) { AIR } else if (rel / 4) % 6 == 5 { STEEL } else { WINDOW_BLUE };
            if b != AIR {
                put(y, b);
            }
        } else if rel % 4 == 0 {
            put(y, if ax % 5 == 2 && az % 5 == 2 { GLOWSTONE } else { CONCRETE_GRAY });
        }
    }
    if dx == 0 && dz == 0 {
        for y in (G + height)..(G + height + 10) {
            put(y, STEEL);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zones_classify() {
        let ny = &CITIES[0];
        assert!(matches!(zone(ny.x, ny.z), Zone::Core(_)));
        assert!(matches!(zone(ny.x, ny.z + ny.radius + 5), Zone::River(_)));
        assert!(matches!(zone(0, 0), Zone::None));
    }

    #[test]
    fn landmarks_reach_the_sky() {
        let mut top = 0;
        tokyo_tower(0, 0, &mut |y, _| top = top.max(y));
        assert!(top > G + 150);
        let mut top = 0;
        eiffel(0, 0, &mut |y, _| top = top.max(y));
        assert!(top > G + 170 && top < 256);
    }
}
