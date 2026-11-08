//! Redstone: levers, buttons, pressure plates, redstone torches and blocks power dust, which
//! carries the signal up to 15 blocks to lamps, doors, TNT and powered rails.

use crate::World;
use runecraft_common::block::{self, BlockId};
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Default)]
pub(crate) struct Redstone {
    parts: HashSet<[i32; 3]>,
    dirty: bool,
    clock: f32,
    buttons: Vec<(f32, [i32; 3])>,
    /// Last power seen by doors and TNT, which react when it changes.
    inputs: HashMap<[i32; 3], bool>,
}

const SIDES: [[i32; 3]; 6] = [[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1]];

fn add(p: [i32; 3], d: [i32; 3]) -> [i32; 3] {
    [p[0] + d[0], p[1] + d[1], p[2] + d[2]]
}

fn is_wire(b: BlockId) -> bool {
    matches!(b, block::WIRE | block::WIRE_ON)
}

impl Redstone {
    pub fn track(&mut self, pos: [i32; 3]) {
        self.parts.insert(pos);
        self.dirty = true;
    }

    /// A redstone block was placed or removed at `pos`.
    pub fn changed(&mut self, pos: [i32; 3], b: BlockId) {
        if block::is_redstone(b) {
            self.parts.insert(pos);
        } else {
            self.parts.remove(&pos);
        }
        if b == block::BUTTON_ON && !self.buttons.iter().any(|x| x.1 == pos) {
            self.buttons.push((1.0, pos));
        }
        self.dirty = true;
    }

    /// Some other block changed: wake up if it's next to a circuit (it may carry power, or be
    /// a door or TNT).
    pub fn neighbour_changed(&mut self, pos: [i32; 3]) {
        if self.parts.is_empty() {
            return;
        }
        for dy in -2..=2 {
            for dz in -2..=2 {
                for dx in -2..=2 {
                    if self.parts.contains(&[pos[0] + dx, pos[1] + dy, pos[2] + dz]) {
                        self.dirty = true;
                        return;
                    }
                }
            }
        }
    }

    pub fn retain_loaded(&mut self, loaded: &HashSet<(i32, i32)>) {
        self.parts.retain(|p| loaded.contains(&(p[0].div_euclid(16), p[2].div_euclid(16))));
    }
}

impl World {
    pub(crate) fn tick_redstone(&mut self, dt: f32) {
        // Buttons pop back up after a second.
        for b in &mut self.lv.rs.buttons {
            b.0 -= dt;
        }
        let due: Vec<[i32; 3]> = self.lv.rs.buttons.iter().filter(|b| b.0 <= 0.0).map(|b| b.1).collect();
        self.lv.rs.buttons.retain(|b| b.0 > 0.0);
        for p in due {
            if self.get(p[0], p[1], p[2]) == block::BUTTON_ON {
                self.set_block(p, block::BUTTON);
            }
        }
        // Pressure plates feel players and vehicles standing on them.
        let plates: Vec<[i32; 3]> = self.lv.rs.parts.iter().copied().filter(|p| matches!(self.get(p[0], p[1], p[2]), block::PLATE | block::PLATE_ON)).collect();
        if !plates.is_empty() {
            let bodies: Vec<[f32; 3]> = self
                .players
                .values()
                .filter(|p| p.dim == self.dim)
                .map(|p| p.pose.pos)
                .chain(self.lv.vehicles.values().map(|v| v.pos))
                .collect();
            for p in plates {
                let on = bodies.iter().any(|b| b[0].floor() as i32 == p[0] && b[2].floor() as i32 == p[2] && (b[1] - p[1] as f32) > -0.2 && (b[1] - p[1] as f32) < 0.8);
                let want = if on { block::PLATE_ON } else { block::PLATE };
                if self.get(p[0], p[1], p[2]) != want {
                    self.set_block(p, want);
                }
            }
        }
        self.lv.rs.clock += dt;
        if self.lv.rs.dirty && self.lv.rs.clock >= 0.1 {
            self.lv.rs.clock = 0.0;
            self.lv.rs.dirty = false;
            self.simulate_redstone();
        }
    }

    fn at(&self, p: [i32; 3]) -> BlockId {
        self.get(p[0], p[1], p[2])
    }

    /// A solid block powered by a source touching it, or by dust on it or pointing into it.
    fn block_powered(&self, s: [i32; 3], power: &HashMap<[i32; 3], u8>) -> bool {
        let b = self.at(s);
        if b == block::REDSTONE_BLOCK {
            return true;
        }
        if !block::is_opaque(b) {
            return false;
        }
        SIDES.iter().any(|d| {
            let m = add(s, *d);
            let mb = self.at(m);
            match mb {
                block::LEVER_ON | block::BUTTON_ON | block::PLATE_ON => true,
                // A torch powers the block above it, not the one it stands on.
                block::RTORCH => d[1] == -1,
                _ if is_wire(mb) => d[1] >= 0 && power.get(&m).copied().unwrap_or(0) > 0,
                _ => false,
            }
        })
    }

    /// Whether something at `c` (a lamp, door, TNT or rail) receives power.
    fn powered_at(&self, c: [i32; 3], power: &HashMap<[i32; 3], u8>) -> bool {
        SIDES.iter().any(|d| {
            let n = add(c, *d);
            let b = self.at(n);
            if b == block::RTORCH {
                // Not into the block the torch stands on.
                return d[1] != 1;
            }
            if block::is_power_source(b) {
                return true;
            }
            if is_wire(b) {
                return power.get(&n).copied().unwrap_or(0) > 0;
            }
            block::is_opaque(b) && self.block_powered(n, power)
        })
    }

    fn simulate_redstone(&mut self) {
        let parts: Vec<[i32; 3]> = self.lv.rs.parts.iter().copied().collect();
        // Strongly powered blocks: those a source touches directly. They feed dust beside them.
        let mut power: HashMap<[i32; 3], u8> = HashMap::new();
        let mut queue = VecDeque::new();
        let feed = |p: [i32; 3], power: &mut HashMap<[i32; 3], u8>, queue: &mut VecDeque<[i32; 3]>| {
            if power.get(&p).copied().unwrap_or(0) < 15 {
                power.insert(p, 15);
                queue.push_back(p);
            }
        };
        for &p in &parts {
            let b = self.at(p);
            if !is_wire(b) {
                continue;
            }
            for d in SIDES {
                let n = add(p, d);
                let nb = self.at(n);
                let strong = match nb {
                    _ if block::is_power_source(nb) => true,
                    // A block with a lever, button or plate on it, or a torch under it.
                    _ if block::is_opaque(nb) => SIDES.iter().any(|e| {
                        let m = add(n, *e);
                        match self.at(m) {
                            block::LEVER_ON | block::BUTTON_ON | block::PLATE_ON => true,
                            block::RTORCH => e[1] == -1,
                            _ => false,
                        }
                    }),
                    _ => false,
                };
                if strong {
                    feed(p, &mut power, &mut queue);
                    break;
                }
            }
        }
        // Dust passes the signal on, one weaker each block, up and down single steps.
        while let Some(w) = queue.pop_front() {
            let pw = power[&w];
            if pw <= 1 {
                continue;
            }
            let above_open = !block::is_opaque(self.at(add(w, [0, 1, 0])));
            for (dx, dz) in block::DIRS {
                let side = [w[0] + dx, w[1], w[2] + dz];
                let mut cands = vec![side];
                if above_open {
                    cands.push(add(side, [0, 1, 0]));
                }
                if !block::is_opaque(self.at(side)) {
                    cands.push(add(side, [0, -1, 0]));
                }
                for n in cands {
                    if is_wire(self.at(n)) && power.get(&n).copied().unwrap_or(0) < pw - 1 {
                        power.insert(n, pw - 1);
                        queue.push_back(n);
                    }
                }
            }
        }
        let mut changes: Vec<([i32; 3], BlockId)> = Vec::new();
        let mut doors: HashSet<[i32; 3]> = HashSet::new();
        let mut tnt: HashSet<[i32; 3]> = HashSet::new();
        for &p in &parts {
            let b = self.at(p);
            let want = match b {
                block::WIRE | block::WIRE_ON => {
                    if power.get(&p).copied().unwrap_or(0) > 0 { block::WIRE_ON } else { block::WIRE }
                }
                block::LAMP | block::LAMP_ON => {
                    if self.powered_at(p, &power) { block::LAMP_ON } else { block::LAMP }
                }
                block::RTORCH | block::RTORCH_OFF => {
                    let base = add(p, [0, -1, 0]);
                    if self.block_powered(base, &power) { block::RTORCH_OFF } else { block::RTORCH }
                }
                _ if block::is_powered_rail(b) => {
                    let on = self.powered_at(p, &power) || self.powered_at(add(p, [0, -1, 0]), &power);
                    let ew = matches!(b, block::POWERED_RAIL_EW | block::POWERED_RAIL_EW_ON);
                    block::powered_rail(ew, on)
                }
                _ => b,
            };
            if want != b {
                changes.push((p, want));
            }
            // Doors and TNT within reach of this part.
            if is_wire(b) || block::is_power_source(b) || matches!(b, block::LEVER | block::BUTTON | block::PLATE | block::RTORCH_OFF) {
                for dy in -2..=2 {
                    for dz in -2..=2 {
                        for dx in -2..=2 {
                            let q = [p[0] + dx, p[1] + dy, p[2] + dz];
                            match self.at(q) {
                                block::TNT => {
                                    tnt.insert(q);
                                }
                                d if block::door_parts(d).is_some_and(|x| !x.0) => {
                                    doors.insert(q);
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
        for (p, b) in changes {
            self.set_block(p, b);
        }
        for d in doors {
            let on = self.powered_at(d, &power) || self.powered_at(add(d, [0, 1, 0]), &power);
            let last = self.lv.rs.inputs.insert(d, on).unwrap_or(false);
            if on != last
                && let Some((false, open, axis)) = block::door_parts(self.at(d))
                && open != on
            {
                self.set_block(d, block::door(false, on, axis));
            }
        }
        for t in tnt {
            let on = self.powered_at(t, &power);
            let last = self.lv.rs.inputs.insert(t, on).unwrap_or(false);
            if on && !last {
                self.set_block(t, block::AIR);
                self.lv.fuses.push((3.0, t));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::*;

    fn flat_world() -> World {
        let dir = std::env::temp_dir().join(format!("runecraft-rs-{}-{:?}", std::process::id(), std::thread::current().id()));
        let mut w = World::open(Config { world_dir: dir, ..Default::default() });
        let mut col = Column::default();
        for x in 0..16 {
            for z in 0..16 {
                col.set(x, 10, z, block::STONE);
            }
        }
        for cz in -1..=1 {
            for cx in -1..=1 {
                w.lv.columns.insert((cx, cz), Loaded { col: col.clone(), encoded: None });
            }
        }
        w
    }

    fn settle(w: &mut World) {
        for _ in 0..40 {
            w.tick_redstone(0.05);
        }
    }

    #[test]
    fn a_lever_lights_a_lamp_down_a_wire_and_a_torch_inverts() {
        let mut w = flat_world();
        w.set_block([2, 11, 2], block::LEVER);
        for x in 3..9 {
            w.set_block([x, 11, 2], block::WIRE);
        }
        w.set_block([9, 11, 2], block::LAMP);
        settle(&mut w);
        assert_eq!(w.get(9, 11, 2), block::LAMP, "off while the lever is off");
        w.set_block([2, 11, 2], block::LEVER_ON);
        settle(&mut w);
        assert_eq!(w.get(5, 11, 2), block::WIRE_ON, "the dust carries power");
        assert_eq!(w.get(9, 11, 2), block::LAMP_ON, "and the lamp lights");

        // A torch on a block powered by the wire turns off.
        w.set_block([9, 11, 2], block::STONE);
        w.set_block([9, 12, 2], block::RTORCH);
        settle(&mut w);
        assert_eq!(w.get(9, 12, 2), block::RTORCH_OFF);
        w.set_block([2, 11, 2], block::LEVER);
        settle(&mut w);
        assert_eq!(w.get(9, 12, 2), block::RTORCH, "and back on when the power goes");
        assert_eq!(w.get(5, 11, 2), block::WIRE);
    }

    #[test]
    fn a_button_opens_a_door_then_closes_it() {
        let mut w = flat_world();
        w.set_block([5, 11, 5], block::door(false, false, 0));
        w.set_block([5, 12, 5], block::door(true, false, 0));
        w.set_block([6, 11, 5], block::BUTTON_ON);
        for _ in 0..4 {
            w.tick_redstone(0.05);
        }
        assert_eq!(block::door_parts(w.get(5, 11, 5)), Some((false, true, 0)), "pressed: the door opens");
        assert_eq!(block::door_parts(w.get(5, 12, 5)), Some((true, true, 0)), "both halves");
        settle(&mut w);
        assert_eq!(w.get(6, 11, 5), block::BUTTON, "the button pops back up");
        assert_eq!(block::door_parts(w.get(5, 11, 5)), Some((false, false, 0)), "and the door closes");
    }
}
