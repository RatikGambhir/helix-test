//! Flowing water and lava: levels spread out and down, drain without a source, and turn into
//! obsidian or cobblestone where they meet.

use crate::World;
use runecraft_common::block::{self, BlockId, Kind};

const H: [[i32; 3]; 4] = [[1, 0, 0], [-1, 0, 0], [0, 0, 1], [0, 0, -1]];
const ALL: [[i32; 3]; 6] = [[1, 0, 0], [-1, 0, 0], [0, 0, 1], [0, 0, -1], [0, 1, 0], [0, -1, 0]];

fn add(p: [i32; 3], d: [i32; 3]) -> [i32; 3] {
    [p[0] + d[0], p[1] + d[1], p[2] + d[2]]
}

/// Blocks fluid washes away.
fn replaceable(b: BlockId) -> bool {
    b == block::AIR || block::def(b).kind == Kind::Plant || matches!(b, block::TORCH | block::RAIL)
}

impl World {
    /// A block changed: it and its neighbours may need to flow.
    pub(crate) fn wake_fluids(&mut self, p: [i32; 3]) {
        for d in ALL.iter().chain(std::iter::once(&[0, 0, 0])) {
            let q = add(p, *d);
            if block::fluid(self.get(q[0], q[1], q[2])).is_some() {
                self.lv.fluid_active.insert(q);
            }
        }
    }

    pub(crate) fn tick_fluids(&mut self, dt: f32) {
        self.lv.fluid_clock.0 += dt;
        self.lv.fluid_clock.1 += dt;
        let water = self.lv.fluid_clock.0 >= 0.25;
        let lava = self.lv.fluid_clock.1 >= 1.0;
        if water {
            self.lv.fluid_clock.0 = 0.0;
        }
        if lava {
            self.lv.fluid_clock.1 = 0.0;
        }
        if !water && !lava {
            return;
        }
        let mut due = Vec::new();
        let mut keep = std::collections::HashSet::new();
        let active: Vec<[i32; 3]> = self.lv.fluid_active.drain().collect();
        for p in active {
            let kind = block::fluid(self.get_raw(p)).map(|f| f.0);
            match kind {
                Some(0) if water && due.len() < 600 => due.push(p),
                Some(1) if lava && due.len() < 600 => due.push(p),
                Some(_) => {
                    keep.insert(p);
                }
                None => {}
            }
        }
        self.lv.fluid_active = keep;
        for p in due {
            self.update_fluid(p);
        }
    }

    fn get_raw(&self, p: [i32; 3]) -> BlockId {
        self.get(p[0], p[1], p[2])
    }

    fn update_fluid(&mut self, p: [i32; 3]) {
        let b = self.get_raw(p);
        let Some((kind, mut level)) = block::fluid(b) else { return };
        let max = if kind == 0 { 8 } else { 4 };
        // Lava touching water hardens.
        if kind == 1 {
            if ALL.iter().any(|d| block::fluid(self.get_raw(add(p, *d))).is_some_and(|f| f.0 == 0)) {
                self.set_block(p, if block::is_source(b) { block::OBSIDIAN } else { block::COBBLE });
                return;
            }
        } else {
            for d in ALL {
                let q = add(p, d);
                if block::fluid(self.get_raw(q)).is_some_and(|f| f.0 == 1) {
                    self.lv.fluid_active.insert(q);
                }
            }
        }
        if !block::is_source(b) {
            // How much the neighbours can feed this cell.
            let above = block::fluid(self.get_raw(add(p, [0, 1, 0]))).is_some_and(|f| f.0 == kind);
            let mut support = if above { max - 1 } else { 0 };
            for d in H {
                let q = add(p, d);
                if let Some((k, l)) = block::fluid(self.get_raw(q))
                    && k == kind
                    && !replaceable(self.get_raw(add(q, [0, -1, 0])))
                {
                    let feed = if block::is_source(self.get_raw(q)) { max - 1 } else { l.saturating_sub(1) };
                    support = support.max(feed);
                }
            }
            if support < level {
                if support == 0 {
                    self.set_block(p, block::AIR);
                    return;
                }
                self.set_block(p, block::fluid_block(kind, support));
                level = support;
            } else if support > level {
                self.set_block(p, block::fluid_block(kind, support));
                level = support;
            }
        }
        let below = add(p, [0, -1, 0]);
        let bb = self.get_raw(below);
        if replaceable(bb) {
            self.set_block(below, block::fluid_block(kind, max - 1));
            return;
        }
        if let Some((k, l)) = block::fluid(bb) {
            if k == kind && !block::is_source(bb) && l < max - 1 {
                self.set_block(below, block::fluid_block(kind, max - 1));
            }
            return;
        }
        let next = if block::is_source(b) { max - 1 } else { level.saturating_sub(1) };
        if next == 0 {
            return;
        }
        for d in H {
            let q = add(p, d);
            let qb = self.get_raw(q);
            let thinner = block::fluid(qb).is_some_and(|(k, l)| k == kind && !block::is_source(qb) && l < next);
            if replaceable(qb) || thinner {
                self.set_block(q, block::fluid_block(kind, next));
            }
        }
    }
}
