//! Portals: a frame of obsidian (to the Ember realm) or glowstone (to the Sky realm), lit with
//! flint and steel. Stand in one for a couple of seconds to cross; a portal is built for you on
//! the other side if there isn't one near.

use crate::World;
use runecraft_common::block::{self, BlockId};
use runecraft_common::chunk::{CHUNK, HEIGHT};
use runecraft_common::worldgen;

const MAX_SIDE: i32 = 21;

fn portal_for(frame: BlockId) -> Option<BlockId> {
    match frame {
        block::OBSIDIAN => Some(block::EMBER_PORTAL),
        block::GLOWSTONE => Some(block::SKY_PORTAL),
        _ => None,
    }
}

fn frame_for(portal: BlockId) -> BlockId {
    if portal == block::SKY_PORTAL { block::GLOWSTONE } else { block::OBSIDIAN }
}

impl World {
    /// Flint and steel struck in the empty cell `pos`.
    pub(crate) fn light(&mut self, pos: [i32; 3]) {
        if self.get(pos[0], pos[1], pos[2]) == block::TNT {
            self.set_block(pos, block::AIR);
            self.lv.fuses.push((3.0, pos));
            return;
        }
        for axis in [0usize, 2] {
            if let Some((cells, portal)) = self.find_frame(pos, axis) {
                for c in cells {
                    self.set_block(c, portal);
                }
                return;
            }
        }
    }

    /// The empty inside of a portal frame around `pos`, lying along `axis` (0 x, 2 z).
    fn find_frame(&self, pos: [i32; 3], axis: usize) -> Option<(Vec<[i32; 3]>, BlockId)> {
        let at = |p: [i32; 3]| self.get(p[0], p[1], p[2]);
        if at(pos) != block::AIR {
            return None;
        }
        let step = |p: [i32; 3], a: usize, d: i32| {
            let mut q = p;
            q[a] += d;
            q
        };
        // Down to the bottom of the frame.
        let mut bottom = pos;
        for _ in 0..MAX_SIDE {
            if at(step(bottom, 1, -1)) != block::AIR {
                break;
            }
            bottom = step(bottom, 1, -1);
        }
        let frame = at(step(bottom, 1, -1));
        let portal = portal_for(frame)?;
        // Out to each side.
        let mut left = bottom;
        while at(step(left, axis, -1)) == block::AIR {
            left = step(left, axis, -1);
            if (left[axis] - bottom[axis]).abs() > MAX_SIDE {
                return None;
            }
        }
        let mut width = 1;
        while at(step(left, axis, width)) == block::AIR {
            width += 1;
            if width > MAX_SIDE {
                return None;
            }
        }
        let mut height = 1;
        while at(step(left, 1, height)) == block::AIR {
            height += 1;
            if height > MAX_SIDE {
                return None;
            }
        }
        if width < 2 || height < 3 {
            return None;
        }
        let mut cells = Vec::new();
        for h in -1..=height {
            for w in -1..=width {
                let c = step(step(left, axis, w), 1, h);
                let edge = h == -1 || h == height || w == -1 || w == width;
                let corner = (h == -1 || h == height) && (w == -1 || w == width);
                if corner {
                    continue;
                }
                if edge {
                    if at(c) != frame {
                        return None;
                    }
                } else if at(c) != block::AIR {
                    return None;
                } else {
                    cells.push(c);
                }
            }
        }
        Some((cells, portal))
    }

    /// A portal block or its frame was broken: the portal it belonged to goes out.
    pub(crate) fn break_portal(&mut self, pos: [i32; 3]) {
        let mut stack: Vec<[i32; 3]> = Vec::new();
        for d in [[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1]] {
            stack.push([pos[0] + d[0], pos[1] + d[1], pos[2] + d[2]]);
        }
        let mut n = 0;
        while let Some(p) = stack.pop() {
            if n > 1000 || !matches!(self.get(p[0], p[1], p[2]), block::EMBER_PORTAL | block::SKY_PORTAL) {
                continue;
            }
            n += 1;
            self.set_block(p, block::AIR);
            for d in [[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1]] {
                stack.push([p[0] + d[0], p[1] + d[1], p[2] + d[2]]);
            }
        }
    }

    pub(crate) fn tick_portals(&mut self, dt: f32) {
        let mut going = Vec::new();
        for (id, p) in self.players.iter_mut().filter(|(_, p)| p.dim == self.dim) {
            let [x, y, z] = p.pose.pos;
            let cell = |dy: f32| (x.floor() as i32, (y + dy).floor() as i32, z.floor() as i32);
            let inside = [0.2, 1.0].iter().map(|dy| cell(*dy)).find_map(|(x, y, z)| {
                let b = if (0..HEIGHT).contains(&y) {
                    let ((cx, lx), (cz, lz)) = (runecraft_common::chunk::split(x), runecraft_common::chunk::split(z));
                    self.lv.columns.get(&(cx, cz)).map_or(block::AIR, |l| l.col.get(lx, y, lz))
                } else {
                    block::AIR
                };
                matches!(b, block::EMBER_PORTAL | block::SKY_PORTAL).then_some(b)
            });
            match inside {
                Some(kind) if !p.portal.1 => {
                    p.portal.0 += dt;
                    if p.portal.0 > 2.0 {
                        p.portal = (0.0, true);
                        let target = match (kind, self.dim) {
                            (block::EMBER_PORTAL, 1) | (block::SKY_PORTAL, 2) => 0,
                            (block::EMBER_PORTAL, _) => 1,
                            _ => 2,
                        };
                        going.push((*id, target, kind));
                    }
                }
                Some(_) => {}
                None => p.portal = (0.0, false),
            }
        }
        let here = self.dim;
        for (id, target, kind) in going {
            self.cross(id, target, kind);
        }
        self.enter(here);
    }

    /// Sends a player to dimension `target` (the realm command, without a portal).
    pub(crate) fn send_through_portal(&mut self, conn: u32, target: u8) {
        let from = self.players.get(&conn).map_or(0, |p| p.dim);
        if from == target {
            return;
        }
        let kind = if target == 2 || from == 2 { block::SKY_PORTAL } else { block::EMBER_PORTAL };
        self.cross(conn, target, kind);
    }

    fn cross(&mut self, conn: u32, target: u8, kind: BlockId) {
        let Some(p) = self.players.get(&conn) else { return };
        let (from, [x, _, z]) = (p.dim, p.pose.pos);
        let scale = |v: f32| match (from, target) {
            (0, 1) => v / 8.0,
            (1, 0) => v * 8.0,
            _ => v,
        };
        let (tx, tz) = (scale(x).floor() as i32, scale(z).floor() as i32);
        self.enter(target);
        for cz in -1..=1 {
            for cx in -1..=1 {
                self.ensure_column(tx.div_euclid(CHUNK) + cx, tz.div_euclid(CHUNK) + cz);
            }
        }
        let arrival = self.find_portal(tx, tz, kind).unwrap_or_else(|| self.build_portal(tx, tz, kind, target));
        if let Some(p) = self.players.get_mut(&conn) {
            p.portal = (0.0, true);
        }
        let name = ["the overworld", "the Ember realm", "the Sky realm"][target as usize];
        self.tell(conn, format!("You arrive in {name}."));
        self.change_dim(conn, target, arrival);
    }

    /// A portal of the same kind already near (x, z), in the current dimension.
    fn find_portal(&self, x: i32, z: i32, kind: BlockId) -> Option<[f32; 3]> {
        let mut best: Option<(i32, [i32; 3])> = None;
        for dz in -16..=16 {
            for dx in -16..=16 {
                let (px, pz) = (x + dx, z + dz);
                if !self.loaded(px, pz) {
                    continue;
                }
                for y in 1..HEIGHT - 1 {
                    if self.get(px, y, pz) == kind && self.get(px, y - 1, pz) != kind {
                        let d = dx * dx + dz * dz;
                        if best.is_none_or(|b| d < b.0) {
                            best = Some((d, [px, y, pz]));
                        }
                    }
                }
            }
        }
        best.map(|(_, p)| [p[0] as f32 + 0.5, p[1] as f32, p[2] as f32 + 0.5])
    }

    /// Builds a portal (with a small platform) at (x, z) and returns where to stand in it.
    fn build_portal(&mut self, x: i32, z: i32, kind: BlockId, dim: u8) -> [f32; 3] {
        let frame = frame_for(kind);
        let open = |w: &World, y: i32| (-2..=3).all(|dx| (0..=4).all(|dy| !block::is_solid(w.get(x + dx, y + dy, z)) && !block::is_liquid(w.get(x + dx, y + dy, z))));
        let y = match dim {
            1 => (worldgen::EMBER_LAVA + 2..100).find(|&y| block::is_solid(self.get(x, y - 1, z)) && open(self, y)).unwrap_or(64),
            2 => (40..110).rev().find(|&y| block::is_solid(self.get(x, y - 1, z)) && open(self, y)).unwrap_or(82),
            _ => (1..HEIGHT - 6).rev().find(|&y| block::is_solid(self.get(x, y - 1, z)) || block::is_liquid(self.get(x, y - 1, z))).unwrap_or(70),
        };
        let floor = match dim {
            1 => block::OBSIDIAN,
            2 => block::CLOUDSTONE,
            _ => block::COBBLE,
        };
        let base = y - 1;
        for dz in -2..=2 {
            for dx in -2..=3 {
                self.set_block([x + dx, base, z + dz], floor);
                for dy in 1..=5 {
                    if dz.abs() <= 1 {
                        self.set_block([x + dx, base + dy, z + dz], block::AIR);
                    }
                }
            }
        }
        // Frame 4 wide and 5 tall along x; the inside fills with portal.
        for dy in 0..=4 {
            for dx in -1..=2 {
                let edge = dy == 0 || dy == 4 || dx == -1 || dx == 2;
                self.set_block([x + dx, base + dy, z], if edge { frame } else { kind });
            }
        }
        [x as f32 + 1.0, y as f32, z as f32 + 0.5]
    }
}

#[cfg(test)]
mod tests {
    use crate::*;

    #[test]
    fn an_obsidian_frame_lights_into_a_portal() {
        let dir = std::env::temp_dir().join(format!("runecraft-portal-{}", std::process::id()));
        let mut w = World::open(Config { world_dir: dir, ..Default::default() });
        let mut col = Column::default();
        for x in 0..16 {
            for z in 0..16 {
                col.set(x, 10, z, block::STONE);
            }
        }
        w.lv.columns.insert((0, 0), Loaded { col, encoded: None });
        for dy in 0..5 {
            for dx in 0..4 {
                if dy == 0 || dy == 4 || dx == 0 || dx == 3 {
                    w.set_block([2 + dx, 11 + dy, 5], block::OBSIDIAN);
                }
            }
        }
        w.light([3, 12, 5]);
        assert_eq!(w.get(3, 12, 5), block::EMBER_PORTAL);
        assert_eq!(w.get(4, 14, 5), block::EMBER_PORTAL);
        w.set_block([2, 13, 5], block::AIR);
        assert_eq!(w.get(3, 12, 5), block::AIR, "breaking the frame puts it out");
    }
}
