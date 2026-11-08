//! Dropped item stacks lying in the world: physics, merging and pickup.

use crate::World;
use glam::Vec3;
use runecraft_common::block::{self, Stack};
use runecraft_common::protocol::{ItemState, ToClient};

pub(crate) struct Item {
    pub stack: Stack,
    pub pos: Vec3,
    vel: Vec3,
    age: f32,
    pickup_delay: f32,
}

impl World {
    pub(crate) fn spawn_item(&mut self, stack: Stack, pos: Vec3, vel: Vec3, delay: f32) {
        if stack.n == 0 || stack.id == block::AIR {
            return;
        }
        let id = self.next_item;
        self.next_item += 1;
        self.items.insert(id, Item { stack, pos: pos - Vec3::Y * 0.1, vel, age: 0.0, pickup_delay: delay });
    }

    fn point_solid(&self, p: Vec3) -> bool {
        let b = self.get(p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
        let f = p - p.floor();
        block::collision_boxes(b).iter().any(|bx| f.x >= bx[0] && f.x <= bx[3] && f.y >= bx[1] && f.y <= bx[4] && f.z >= bx[2] && f.z <= bx[5])
    }

    /// Falling, sliding and floating, one step.
    fn step_item(&self, it: &mut Item, dt: f32) {
        it.vel.y -= 18.0 * dt;
        let in_water = block::fluid(self.get(it.pos.x.floor() as i32, it.pos.y.floor() as i32, it.pos.z.floor() as i32)).is_some();
        if in_water {
            it.vel.y = it.vel.y.max(-1.0) + 12.0 * dt;
            it.vel *= 1.0 - (2.0 * dt).min(1.0);
        }
        for axis in [1usize, 0, 2] {
            let mut next = it.pos;
            next[axis] += it.vel[axis] * dt;
            if self.point_solid(next) {
                if axis == 1 && it.vel.y < 0.0 {
                    it.vel.x *= 0.6;
                    it.vel.z *= 0.6;
                }
                it.vel[axis] = 0.0;
            } else {
                it.pos = next;
            }
        }
    }

    pub(crate) fn tick_items(&mut self, dt: f32) {
        let players: Vec<(u32, Vec3)> = self.players.iter().map(|(id, p)| (*id, p.pos())).collect();
        let ids: Vec<u32> = self.items.keys().copied().collect();
        let mut gone = Vec::new();
        for id in ids {
            let Some(mut it) = self.items.remove(&id) else { continue };
            it.age += dt;
            it.pickup_delay -= dt;
            let loaded = self.loaded(it.pos.x.floor() as i32, it.pos.z.floor() as i32);
            if it.age > 300.0 || !loaded || it.pos.y < -10.0 {
                gone.push(id);
                continue;
            }
            self.step_item(&mut it, dt);
            // Walk over it to pick it up.
            if it.pickup_delay <= 0.0
                && let Some((pid, d)) = players.iter().map(|(pid, p)| (*pid, (*p + Vec3::Y * 0.6).distance(it.pos))).min_by(|a, b| a.1.total_cmp(&b.1))
                && d < 1.8
            {
                if let Some(p) = self.players.get(&pid) {
                    p.send(&ToClient::Pickup(it.stack));
                }
                gone.push(id);
                continue;
            }
            self.items.insert(id, it);
        }
        for id in gone {
            self.items.remove(&id);
        }
        self.merge_clock += dt;
        if self.merge_clock > 1.0 {
            self.merge_clock = 0.0;
            self.merge_items();
        }
        for p in self.players.values() {
            let at = p.pos();
            let list: Vec<ItemState> = self.items.iter().filter(|(_, it)| it.pos.distance(at) < 80.0).map(|(id, it)| ItemState { id: *id, item: it.stack.id, pos: it.pos.to_array() }).collect();
            p.send(&ToClient::Items(list));
        }
    }

    /// Stacks of the same thing lying together become one.
    fn merge_items(&mut self) {
        let all: Vec<(u32, Vec3, Stack)> = self.items.iter().map(|(id, it)| (*id, it.pos, it.stack)).collect();
        let mut removed = std::collections::HashSet::new();
        for (i, (a, pa, sa)) in all.iter().enumerate() {
            if removed.contains(a) {
                continue;
            }
            for (b, pb, sb) in all.iter().skip(i + 1) {
                if removed.contains(b) || !sa.same(sb) || pa.distance(*pb) > 1.2 {
                    continue;
                }
                let total = self.items[a].stack.n as u32 + sb.n as u32;
                if total > block::max_stack(sa.id) as u32 {
                    continue;
                }
                if let Some(it) = self.items.get_mut(a) {
                    it.stack.n = total as u8;
                }
                removed.insert(*b);
            }
        }
        for id in removed {
            self.items.remove(&id);
        }
    }
}
