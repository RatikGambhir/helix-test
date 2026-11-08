//! World generation: terrain, biomes, caves, ores, trees and plants. Every column is built
//! from the seed and its own coordinates alone, so server and clients always agree.

use crate::block::*;
use crate::chunk::{CHUNK, Column, HEIGHT, SEA_LEVEL};
use crate::city::{self, Zone};
use crate::noise::{fbm2, hash2, hash3, perlin3, unit};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Biome {
    Ocean,
    Beach,
    Plains,
    Forest,
    BirchForest,
    Desert,
    Snowy,
    Mountains,
    Cherry,
    City,
}

impl Biome {
    pub fn name(self) -> &'static str {
        match self {
            Biome::Ocean => "Ocean",
            Biome::Beach => "Beach",
            Biome::Plains => "Plains",
            Biome::Forest => "Forest",
            Biome::BirchForest => "Birch Forest",
            Biome::Desert => "Desert",
            Biome::Snowy => "Snowy Tundra",
            Biome::Mountains => "Mountains",
            Biome::Cherry => "Cherry Grove",
            Biome::City => "City",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub height: i32,
    pub biome: Biome,
    pub zone: Zone,
}

/// Natural terrain height and biome at a world column, before cities flatten it.
fn natural(seed: u64, x: i32, z: i32) -> (i32, Biome) {
    let (fx, fz) = (x as f32, z as f32);
    let continent = fbm2(seed, fx / 700.0, fz / 700.0, 4);
    let hills = fbm2(seed.wrapping_add(11), fx / 160.0, fz / 160.0, 4);
    let detail = fbm2(seed.wrapping_add(12), fx / 40.0, fz / 40.0, 3);
    let mountain = fbm2(seed.wrapping_add(13), fx / 420.0, fz / 420.0, 3);
    let mut h = 66.0 + continent * 22.0 + hills * 10.0 + detail * 3.0;
    let m = (mountain - 0.18).max(0.0);
    h += m * m * 600.0 + m * hills.abs() * 90.0;
    // Keep spawn on dry land.
    let near_spawn = 1.0 - ((fx * fx + fz * fz).sqrt() / 120.0).min(1.0);
    h = h * (1.0 - near_spawn) + (h.max(68.0)) * near_spawn;
    let h = (h as i32).clamp(4, HEIGHT - 40);

    let temp = fbm2(seed.wrapping_add(21), fx / 900.0, fz / 900.0, 3) - (h - 70).max(0) as f32 / 120.0;
    let wet = fbm2(seed.wrapping_add(22), fx / 750.0, fz / 750.0, 3);
    let biome = if h < SEA_LEVEL - 1 {
        Biome::Ocean
    } else if h > 120 {
        if temp < 0.0 { Biome::Snowy } else { Biome::Mountains }
    } else if h <= SEA_LEVEL + 1 && temp > -0.3 {
        Biome::Beach
    } else if temp < -0.32 {
        Biome::Snowy
    } else if temp > 0.28 && wet < 0.0 {
        Biome::Desert
    } else if temp > 0.1 && wet > 0.3 {
        Biome::Cherry
    } else if wet > 0.12 {
        if temp < 0.0 { Biome::BirchForest } else { Biome::Forest }
    } else {
        Biome::Plains
    };
    (h, biome)
}

pub fn sample(seed: u64, x: i32, z: i32) -> Sample {
    let zone = city::zone(x, z);
    let (h, b) = natural(seed, x, z);
    match zone {
        Zone::None => Sample { height: h, biome: b, zone },
        Zone::Core(_) => Sample { height: city::GROUND, biome: Biome::City, zone },
        Zone::River(_) => Sample { height: SEA_LEVEL - 6, biome: Biome::Ocean, zone },
        Zone::Blend(_, t) => {
            let blended = city::GROUND as f32 + (h - city::GROUND) as f32 * t;
            let biome = if t < 0.3 { Biome::Plains } else { b };
            Sample { height: blended.round() as i32, biome, zone }
        }
    }
}

fn surface_blocks(b: Biome, h: i32) -> (BlockId, BlockId) {
    match b {
        Biome::Ocean => {
            if h < SEA_LEVEL - 12 { (GRAVEL, GRAVEL) } else { (SAND, SAND) }
        }
        Biome::Beach | Biome::Desert => (SAND, SAND),
        Biome::Snowy => (SNOW, DIRT),
        Biome::Mountains => {
            if h > 150 { (SNOW, STONE) } else { (STONE, STONE) }
        }
        _ => (GRASS, DIRT),
    }
}

pub fn generate(seed: u64, cx: i32, cz: i32) -> Column {
    let mut col = Column::default();
    let mut samples = [[Sample { height: 0, biome: Biome::Plains, zone: Zone::None }; 16]; 16];
    for lz in 0..CHUNK {
        for lx in 0..CHUNK {
            let (x, z) = (cx * CHUNK + lx, cz * CHUNK + lz);
            let s = sample(seed, x, z);
            samples[lz as usize][lx as usize] = s;
            let h = s.height;
            let (top, under) = surface_blocks(s.biome, h);
            let cave_ok = matches!(s.zone, Zone::None);
            for y in 0..=h.max(SEA_LEVEL) {
                let b = if y == 0 || (y < 4 && unit(hash3(seed, x, y, z)) < 0.5) {
                    BEDROCK
                } else if y > h {
                    if s.biome == Biome::Snowy && y == SEA_LEVEL { ICE } else { WATER }
                } else if y == h {
                    if top == GRASS && h < SEA_LEVEL { DIRT } else { top }
                } else if y > h - 4 {
                    if s.biome == Biome::Desert && y < h - 2 { SANDSTONE } else { under }
                } else {
                    ore(seed, x, y, z)
                };
                col.set_raw(lx, y, lz, b);
            }
            if cave_ok {
                carve_caves(seed, &mut col, lx, lz, x, z, h);
            }
            if let Zone::Core(c) | Zone::River(c) = s.zone {
                city::fill_column(seed, c, x, z, &mut |y, b| col.set_raw(lx, y, lz, b));
            }
            if let Zone::None | Zone::Blend(_, _) = s.zone {
                plants(seed, &mut col, lx, lz, x, z, &s);
            }
        }
    }
    trees(seed, &mut col, cx, cz);
    col.recompute_heights();
    col
}

fn ore(seed: u64, x: i32, y: i32, z: i32) -> BlockId {
    // Ores come in small clumps: a 2×2×2 cell decides, then each block in it rolls.
    let cell = unit(hash3(seed ^ 0x0E5, x >> 1, y >> 1, z >> 1));
    let roll = unit(hash3(seed ^ 0x0E6, x, y, z));
    if roll < 0.55 {
        if y < 14 && cell < 0.012 {
            return DIAMOND_ORE;
        }
        if y < 32 && cell < 0.02 {
            return GOLD_ORE;
        }
        if y < 64 && cell < 0.05 {
            return IRON_ORE;
        }
        if y < 128 && cell < 0.09 {
            return COAL_ORE;
        }
    }
    if y < 40 && unit(hash3(seed ^ 0x6A1, x >> 2, y >> 2, z >> 2)) < 0.03 {
        return GRAVEL;
    }
    STONE
}

fn carve_caves(seed: u64, col: &mut Column, lx: i32, lz: i32, x: i32, z: i32, h: i32) {
    let top = (h - 1).min(HEIGHT - 2);
    for y in 5..top {
        let (fx, fy, fz) = (x as f32, y as f32, z as f32);
        let a = perlin3(seed ^ 0xCA5E, fx / 48.0, fy / 28.0, fz / 48.0);
        if a.abs() > 0.09 {
            continue;
        }
        let b = perlin3(seed ^ 0xCA5F, fx / 48.0, fy / 28.0, fz / 48.0);
        if b.abs() > 0.09 {
            continue;
        }
        // Don't open caves under the sea: it floods the world.
        if h < SEA_LEVEL + 2 && y > h - 6 {
            continue;
        }
        let cur = col.get(lx, y, lz);
        if cur == BEDROCK || cur == WATER {
            continue;
        }
        col.set_raw(lx, y, lz, if y < 11 { LAVA } else { AIR });
    }
}

fn plants(seed: u64, col: &mut Column, lx: i32, lz: i32, x: i32, z: i32, s: &Sample) {
    let h = s.height;
    if h < SEA_LEVEL || h + 1 >= HEIGHT || col.get(lx, h, lz) != GRASS && col.get(lx, h, lz) != SAND {
        return;
    }
    let r = unit(hash2(seed ^ 0x9A55, x, z));
    let above = match s.biome {
        Biome::Desert if r < 0.006 => {
            let tall = 1 + (hash2(seed, x, z) % 3) as i32;
            for i in 1..=tall {
                col.set_raw(lx, h + i, lz, CACTUS);
            }
            return;
        }
        Biome::Plains | Biome::Cherry if r < 0.12 => TALL_GRASS,
        Biome::Plains | Biome::Cherry if r < 0.14 => FLOWER_RED,
        Biome::Plains | Biome::Cherry if r < 0.16 => FLOWER_YELLOW,
        Biome::Forest | Biome::BirchForest if r < 0.08 => TALL_GRASS,
        Biome::Forest | Biome::BirchForest if r < 0.09 => FLOWER_RED,
        _ => return,
    };
    if col.get(lx, h, lz) == GRASS {
        col.set_raw(lx, h + 1, lz, above);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TreeKind {
    Oak,
    Birch,
    Spruce,
    Cherry,
}

/// A tree rooted on the ground block at (x, y, z).
pub struct Tree {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub kind: TreeKind,
    pub height: i32,
}

const TREE_CELL: i32 = 5;

fn tree_in_cell(seed: u64, gx: i32, gz: i32) -> Option<Tree> {
    let hsh = hash2(seed ^ 0x7EE5, gx, gz);
    let x = gx * TREE_CELL + (hsh % TREE_CELL as u64) as i32;
    let z = gz * TREE_CELL + ((hsh >> 8) % TREE_CELL as u64) as i32;
    let s = sample(seed, x, z);
    if !matches!(s.zone, Zone::None) {
        return None;
    }
    let density = match s.biome {
        Biome::Forest => 0.7,
        Biome::BirchForest => 0.6,
        Biome::Cherry => 0.35,
        Biome::Snowy => 0.12,
        Biome::Plains => 0.03,
        Biome::Mountains => 0.05,
        _ => 0.0,
    };
    if unit(hsh >> 16) >= density || s.height < SEA_LEVEL + 1 || s.height > 150 {
        return None;
    }
    let kind = match s.biome {
        Biome::BirchForest => TreeKind::Birch,
        Biome::Snowy | Biome::Mountains => TreeKind::Spruce,
        Biome::Cherry => TreeKind::Cherry,
        Biome::Forest if (hsh >> 40) % 5 == 0 => TreeKind::Birch,
        _ => TreeKind::Oak,
    };
    let height = match kind {
        TreeKind::Spruce => 6 + ((hsh >> 44) % 4) as i32,
        TreeKind::Birch => 5 + ((hsh >> 44) % 3) as i32,
        _ => 4 + ((hsh >> 44) % 3) as i32,
    };
    Some(Tree { x, y: s.height, z, kind, height })
}

/// Calls `put` for every block of a tree, in world coordinates.
pub fn tree_blocks(seed: u64, t: &Tree, put: &mut dyn FnMut(i32, i32, i32, BlockId, bool)) {
    let (log, leaves) = match t.kind {
        TreeKind::Oak => (OAK_LOG, OAK_LEAVES),
        TreeKind::Birch => (BIRCH_LOG, BIRCH_LEAVES),
        TreeKind::Spruce => (SPRUCE_LOG, SPRUCE_LEAVES),
        TreeKind::Cherry => (OAK_LOG, CHERRY_LEAVES),
    };
    let top = t.y + t.height;
    match t.kind {
        TreeKind::Spruce => {
            for y in (t.y + 2)..=(top + 1) {
                let r = ((top + 1 - y) as f32 * 0.45).ceil() as i32 % 4;
                let r = if (top - y) % 2 == 0 { r.max(1) } else { (r - 1).max(0) };
                for dz in -r..=r {
                    for dx in -r..=r {
                        if dx * dx + dz * dz <= r * r + 1 {
                            put(t.x + dx, y, t.z + dz, leaves, false);
                        }
                    }
                }
            }
            put(t.x, top + 2, t.z, leaves, false);
        }
        TreeKind::Cherry => {
            for dy in -1..=2 {
                let r: i32 = if dy == 2 { 2 } else { 3 };
                for dz in -r..=r {
                    for dx in -r..=r {
                        let d = dx * dx + dz * dz;
                        if d <= r * r && !(d == r * r && unit(hash3(seed, t.x + dx, top + dy, t.z + dz)) < 0.5) {
                            put(t.x + dx, top + dy, t.z + dz, leaves, false);
                        }
                    }
                }
            }
        }
        _ => {
            for dy in -2..=1 {
                let r: i32 = if dy >= 0 { 1 } else { 2 };
                for dz in -r..=r {
                    for dx in -r..=r {
                        let corner = dx.abs() == r && dz.abs() == r;
                        if corner && (dy == 1 || unit(hash3(seed, t.x + dx, top + dy, t.z + dz)) < 0.6) {
                            continue;
                        }
                        put(t.x + dx, top + dy, t.z + dz, leaves, false);
                    }
                }
            }
        }
    }
    for y in (t.y + 1)..=top {
        put(t.x, y, t.z, log, true);
    }
    put(t.x, t.y, t.z, DIRT, true);
}

fn trees(seed: u64, col: &mut Column, cx: i32, cz: i32) {
    let (x0, z0) = (cx * CHUNK, cz * CHUNK);
    let g0x = (x0 - 4).div_euclid(TREE_CELL);
    let g1x = (x0 + CHUNK + 4).div_euclid(TREE_CELL);
    let g0z = (z0 - 4).div_euclid(TREE_CELL);
    let g1z = (z0 + CHUNK + 4).div_euclid(TREE_CELL);
    for gz in g0z..=g1z {
        for gx in g0x..=g1x {
            let Some(t) = tree_in_cell(seed, gx, gz) else { continue };
            tree_blocks(seed, &t, &mut |x, y, z, b, force| {
                let (lx, lz) = (x - x0, z - z0);
                if (0..CHUNK).contains(&lx) && (0..CHUNK).contains(&lz) && (0..HEIGHT).contains(&y) {
                    let cur = col.get(lx, y, lz);
                    if force || cur == AIR || cur == TALL_GRASS {
                        col.set_raw(lx, y, lz, b);
                    }
                }
            });
        }
    }
}

/// Where a player should stand at a column: the first air above the ground.
pub fn spawn_height(seed: u64, x: i32, z: i32) -> i32 {
    let col = generate(seed, x.div_euclid(CHUNK), z.div_euclid(CHUNK));
    let (lx, lz) = (x.rem_euclid(CHUNK), z.rem_euclid(CHUNK));
    let mut y = HEIGHT - 2;
    while y > 0 && !is_solid(col.get(lx, y, lz)) && !is_liquid(col.get(lx, y, lz)) {
        y -= 1;
    }
    y + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generation_is_deterministic_and_has_ground() {
        let a = generate(42, 0, 0);
        let b = generate(42, 0, 0);
        assert!(a.blocks == b.blocks);
        assert_eq!(a.get(0, 0, 0), BEDROCK);
        assert!(spawn_height(42, 0, 0) > SEA_LEVEL);
    }

    #[test]
    fn every_city_generates() {
        for c in city::CITIES.iter() {
            let col = generate(7, c.x.div_euclid(16), c.z.div_euclid(16));
            assert!(col.blocks.iter().any(|&b| b != AIR));
        }
    }
}
