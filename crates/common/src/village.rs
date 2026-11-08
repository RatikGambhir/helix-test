//! Villages: clusters of houses, farms, a well and a market square with trading posts, built
//! on flattened ground in plains, forests and deserts. Nobody lives in them: you trade at the
//! trading posts yourself.

use crate::block::*;
use crate::noise::{hash2, unit};
use std::cell::RefCell;
use std::collections::HashMap;

/// Villages sit one per cell of this many blocks, at most.
pub const CELL: i32 = 640;
/// Blocks from the centre to the end of the roads.
pub const RADIUS: i32 = 40;
/// Width of the ring where village ground blends into the land around it.
pub const BLEND: i32 = 14;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Village {
    pub x: i32,
    pub z: i32,
    /// Ground level of the whole village.
    pub ground: i32,
    pub desert: bool,
    pub salt: u64,
}

/// What a lot holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Lot {
    House,
    Farm,
    Market,
}

#[derive(Clone, Copy, Debug)]
struct Building {
    kind: Lot,
    x0: i32,
    z0: i32,
    x1: i32,
    z1: i32,
    /// Side the door faces: 0 north, 1 east, 2 south, 3 west.
    door: u8,
}

thread_local! {
    static CACHE: RefCell<HashMap<(u64, i32, i32), Option<Village>>> = RefCell::new(HashMap::new());
}

/// The village in a cell, if there is one. `land` gives natural ground height and whether
/// the spot suits a village (and if it's desert).
pub fn in_cell(seed: u64, gx: i32, gz: i32, land: &dyn Fn(i32, i32) -> Option<(i32, bool)>) -> Option<Village> {
    let key = (seed, gx, gz);
    if let Some(v) = CACHE.with(|c| c.borrow().get(&key).copied()) {
        return v;
    }
    let v = find(seed, gx, gz, land);
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if c.len() > 4096 {
            c.clear();
        }
        c.insert(key, v);
    });
    v
}

fn find(seed: u64, gx: i32, gz: i32, land: &dyn Fn(i32, i32) -> Option<(i32, bool)>) -> Option<Village> {
    let h = hash2(seed ^ 0x7111A6E, gx, gz);
    if unit(h) > 0.75 {
        return None;
    }
    let margin = RADIUS + BLEND + 20;
    let span = (CELL - 2 * margin) as u64;
    let x = gx * CELL + margin + ((h >> 8) % span) as i32;
    let z = gz * CELL + margin + ((h >> 24) % span) as i32;
    let (ground, desert) = land(x, z)?;
    // Only on fairly even land: check a ring of points.
    for i in 0..8 {
        let a = i as f32 * std::f32::consts::TAU / 8.0;
        let (px, pz) = (x + (a.cos() * RADIUS as f32) as i32, z + (a.sin() * RADIUS as f32) as i32);
        let (g, _) = land(px, pz)?;
        if (g - ground).abs() > 9 {
            return None;
        }
    }
    Some(Village { x, z, ground, desert, salt: h })
}

/// The village whose grounds include (x, z), if any.
pub fn near(seed: u64, x: i32, z: i32, land: &dyn Fn(i32, i32) -> Option<(i32, bool)>) -> Option<Village> {
    let v = in_cell(seed, x.div_euclid(CELL), z.div_euclid(CELL), land)?;
    let d = (x - v.x).abs().max((z - v.z).abs());
    (d <= RADIUS + BLEND).then_some(v)
}

impl Village {
    /// How strongly the village's flat ground applies at a column: 1 inside, falling to 0
    /// across the blend ring.
    pub fn weight(&self, x: i32, z: i32) -> f32 {
        let d = (x - self.x).abs().max((z - self.z).abs());
        if d <= RADIUS {
            1.0
        } else {
            (1.0 - (d - RADIUS) as f32 / BLEND as f32).clamp(0.0, 1.0)
        }
    }

    /// True where nothing natural (trees, flowers) should grow.
    pub fn claims(&self, x: i32, z: i32) -> bool {
        (x - self.x).abs().max((z - self.z).abs()) <= RADIUS + 2
    }

    fn buildings(&self) -> Vec<Building> {
        let mut out = Vec::new();
        // The market: two trading posts under awnings by the well.
        out.push(Building { kind: Lot::Market, x0: self.x + 3, z0: self.z - 6, x1: self.x + 5, z1: self.z - 4, door: 3 });
        out.push(Building { kind: Lot::Market, x0: self.x - 5, z0: self.z + 4, x1: self.x - 3, z1: self.z + 6, door: 1 });
        for arm in 0..4u8 {
            let (ax, az) = DIRS[arm as usize];
            let (px, pz) = DIRS[((arm + 1) & 3) as usize];
            for k in 0..3 {
                for side in [-1, 1] {
                    let r = unit(hash2(self.salt, arm as i32 * 10 + k, side));
                    let kind = if r < 0.12 {
                        continue;
                    } else if r < 0.4 {
                        Lot::Farm
                    } else {
                        Lot::House
                    };
                    let along = 12 + k * 10;
                    let (cx, cz) = (self.x + ax * along + px * side * 9, self.z + az * along + pz * side * 9);
                    let (hw, hd) = match kind {
                        Lot::Farm => (3, 4),
                        _ => (2 + (r * 10.0) as i32 % 2, 2),
                    };
                    // Long side runs along the road.
                    let (hx, hz) = if ax != 0 { (hd, hw) } else { (hw, hd) };
                    // The door faces back toward the road.
                    let door = if side == 1 { (arm + 3) & 3 } else { (arm + 1) & 3 };
                    out.push(Building { kind, x0: cx - hx, z0: cz - hz, x1: cx + hx, z1: cz + hz, door });
                }
            }
        }
        out
    }

    /// Fills a column of the village at world (x, z), above and at ground level.
    pub fn fill_column(&self, x: i32, z: i32, put: &mut dyn FnMut(i32, BlockId)) {
        let g = self.ground;
        let (dx, dz) = (x - self.x, z - self.z);
        let wall = if self.desert { SANDSTONE } else { PLANKS };
        // Roads: gravel crosses through the middle; a cobbled square in the centre.
        let on_road = (dx.abs() <= 1 && dz.abs() <= RADIUS) || (dz.abs() <= 1 && dx.abs() <= RADIUS);
        let square = dx.abs() <= 7 && dz.abs() <= 7;
        if square {
            put(g, COBBLE);
        } else if on_road {
            put(g, if self.desert { SANDSTONE } else { GRAVEL });
        }
        // Street lamps along the roads.
        if on_road && dx.abs() != dz.abs() {
            let along = dx.abs().max(dz.abs());
            let across = dx.abs().min(dz.abs());
            let side = if dx.abs() > dz.abs() { dz } else { dx };
            if across == 1 && side == 1 && along > 8 && along % 10 == 0 {
                for y in 1..=2 {
                    put(g + y, OAK_LOG);
                }
                put(g + 3, LANTERN);
            }
        }
        // The well, in the middle of the square.
        if dx.abs() <= 1 && dz.abs() <= 1 {
            if dx == 0 && dz == 0 {
                for y in (g - 4)..=g {
                    put(y, WATER);
                }
                put(g - 5, COBBLE);
            } else {
                put(g + 1, COBBLE);
                for y in (g - 4)..=g {
                    put(y, COBBLE);
                }
            }
            if dx.abs() == 1 && dz.abs() == 1 {
                put(g + 2, OAK_LOG);
                put(g + 3, OAK_LOG);
            }
            put(g + 4, if self.desert { SANDSTONE } else { PLANKS });
            return;
        }
        for b in self.buildings() {
            if x < b.x0 || x > b.x1 || z < b.z0 || z > b.z1 {
                continue;
            }
            match b.kind {
                Lot::House => self.house(&b, x, z, wall, put),
                Lot::Farm => {
                    let edge = x == b.x0 || x == b.x1 || z == b.z0 || z == b.z1;
                    let mid = if b.x1 - b.x0 > b.z1 - b.z0 { z == (b.z0 + b.z1) / 2 } else { x == (b.x0 + b.x1) / 2 };
                    if edge {
                        put(g, OAK_LOG);
                    } else if mid {
                        put(g, WATER);
                    } else {
                        put(g, FARMLAND);
                        let stage = (unit(hash2(self.salt ^ 0xFA, x, z)) * 4.0) as u8;
                        put(g + 1, WHEAT_0 + stage.min(3));
                    }
                }
                Lot::Market => {
                    let corner = (x == b.x0 || x == b.x1) && (z == b.z0 || z == b.z1);
                    let centre = x == (b.x0 + b.x1) / 2 && z == (b.z0 + b.z1) / 2;
                    put(g, COBBLE);
                    if corner {
                        put(g + 1, OAK_LOG);
                        put(g + 2, OAK_LOG);
                    }
                    if centre {
                        put(g + 1, TRADING_POST);
                    }
                    // A striped awning.
                    let stripe = if (x + z).rem_euclid(2) == 0 { WOOL_RED } else { WOOL_WHITE };
                    put(g + 3, if self.desert && stripe == WOOL_RED { WOOL_YELLOW } else { stripe });
                    if corner && x == b.x0 && z == b.z0 {
                        put(g + 4, LANTERN);
                    }
                }
            }
            return;
        }
    }

    fn house(&self, b: &Building, x: i32, z: i32, wall: BlockId, put: &mut dyn FnMut(i32, BlockId)) {
        let g = self.ground;
        let edge_x = x == b.x0 || x == b.x1;
        let edge_z = z == b.z0 || z == b.z1;
        let (mx, mz) = ((b.x0 + b.x1) / 2, (b.z0 + b.z1) / 2);
        put(g, if edge_x || edge_z { COBBLE } else { PLANKS });
        for y in (g + 1)..=(g + 3) {
            put(y, AIR);
        }
        if edge_x || edge_z {
            let corner = edge_x && edge_z;
            // Which wall this is, and whether the door is in it.
            let side = if z == b.z0 { 0 } else if x == b.x1 { 1 } else if z == b.z1 { 2 } else { 3 };
            let door_here = !corner && side == b.door && (if side % 2 == 0 { x == mx } else { z == mz });
            for y in 1..=3 {
                let blk = if corner {
                    if self.desert { SANDSTONE } else { OAK_LOG }
                } else if door_here && y <= 2 {
                    door(y == 2, false, if side % 2 == 0 { 0 } else { 1 })
                } else if y == 2 && !door_here && (if side % 2 == 0 { (x - mx).abs() == 1 } else { (z - mz).abs() == 1 }) {
                    GLASS
                } else {
                    wall
                };
                put(g + y, blk);
            }
        } else {
            // Furniture against the back wall.
            let back = (b.door + 2) & 3;
            let at_back = match back {
                0 => z == b.z0 + 1,
                1 => x == b.x1 - 1,
                2 => z == b.z1 - 1,
                _ => x == b.x0 + 1,
            };
            if at_back {
                let first = if back % 2 == 0 { x == b.x0 + 1 } else { z == b.z0 + 1 };
                let last = if back % 2 == 0 { x == b.x1 - 1 } else { z == b.z1 - 1 };
                if first {
                    put(g + 1, BED);
                } else if last {
                    put(g + 1, CRAFTING_TABLE);
                } else {
                    put(g + 1, CHEST);
                }
            } else if (x == b.x0 + 1 || x == b.x1 - 1) && (z == b.z0 + 1 || z == b.z1 - 1) {
                put(g + 1, TORCH);
            }
        }
        // A roof that steps in toward the ridge.
        let inset = (x - b.x0).min(b.x1 - x).min(z - b.z0).min(b.z1 - z);
        let roof = if self.desert { SANDSTONE } else if inset == 0 { SPRUCE_LOG } else { BRICK };
        if self.desert {
            put(g + 4, roof);
        } else {
            for k in 0..=inset {
                put(g + 4 + k, if k == inset { roof } else { PLANKS });
            }
        }
    }
}

/// A trade: give `cost` of something, get `out` of something else.
#[derive(Clone, Copy, Debug)]
pub struct Offer {
    pub cost: (BlockId, u8),
    pub out: (BlockId, u8),
}

const fn o(cost: BlockId, cn: u8, out: BlockId, on: u8) -> Offer {
    Offer { cost: (cost, cn), out: (out, on) }
}

/// What every trading post offers.
pub const OFFERS: &[Offer] = &[
    o(WHEAT, 20, EMERALD, 1),
    o(COAL, 15, EMERALD, 1),
    o(IRON_INGOT, 4, EMERALD, 1),
    o(OAK_LOG, 16, EMERALD, 1),
    o(EMERALD, 1, BREAD, 4),
    o(EMERALD, 1, APPLE, 4),
    o(EMERALD, 1, GLASS, 16),
    o(EMERALD, 1, WIRE, 12),
    o(EMERALD, 2, RAIL, 16),
    o(EMERALD, 3, POWERED_RAIL, 6),
    o(EMERALD, 2, LANTERN, 4),
    o(EMERALD, 3, FLINT_STEEL, 1),
    o(EMERALD, 4, MINECART, 1),
    o(EMERALD, 4, BOAT, 1),
    o(EMERALD, 6, DIAMOND, 1),
    o(EMERALD, 8, CAR, 1),
    o(EMERALD, 12, TRAIN, 1),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn villages_appear_and_hold_a_trading_post() {
        let land = |_x: i32, _z: i32| Some((70, false));
        let mut found = None;
        for g in 0..10 {
            if let Some(v) = in_cell(1, g, 0, &land) {
                found = Some(v);
                break;
            }
        }
        let v = found.expect("some cell has a village");
        let mut posts = 0;
        for x in (v.x - RADIUS)..=(v.x + RADIUS) {
            for z in (v.z - RADIUS)..=(v.z + RADIUS) {
                v.fill_column(x, z, &mut |_, b| {
                    if b == TRADING_POST {
                        posts += 1;
                    }
                });
            }
        }
        assert_eq!(posts, 2);
        assert!(OFFERS.iter().all(|o| o.cost.1 >= 1 && o.out.1 >= 1 && o.out.1 <= max_stack(o.out.0)));
    }
}
