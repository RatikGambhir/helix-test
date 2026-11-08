//! Player movement and collision against the block grid, and block raycasting.

use crate::block::{self, BlockId};
use glam::{IVec3, Vec3};

pub const HALF_W: f32 = 0.3;
pub const HEIGHT: f32 = 1.8;
pub const EYE: f32 = 1.62;
pub const GRAVITY: f32 = 30.0;
pub const JUMP: f32 = 8.6;
pub const WALK: f32 = 4.3;
pub const SPRINT: f32 = 5.8;
pub const FLY: f32 = 11.0;

#[derive(Clone, Copy, Debug, Default)]
pub struct Body {
    /// Feet, at the centre of the footprint.
    pub pos: Vec3,
    pub vel: Vec3,
    pub on_ground: bool,
    pub in_water: bool,
    pub head_in_water: bool,
    pub in_lava: bool,
    fall_from: Option<f32>,
}

pub struct Input {
    /// Desired horizontal velocity, world space.
    pub wish: Vec3,
    pub jump: bool,
    pub descend: bool,
    pub flying: bool,
}


impl Body {
    pub fn new(pos: Vec3) -> Body {
        Body { pos, ..Default::default() }
    }

    fn collides(&self, p: Vec3, get: &dyn Fn(i32, i32, i32) -> BlockId) -> bool {
        let min = p - Vec3::new(HALF_W, 0.0, HALF_W);
        let max = p + Vec3::new(HALF_W, HEIGHT, HALF_W);
        let eps = 1e-4;
        for y in (min.y + eps).floor() as i32..=(max.y - eps).floor() as i32 {
            for z in (min.z + eps).floor() as i32..=(max.z - eps).floor() as i32 {
                for x in (min.x + eps).floor() as i32..=(max.x - eps).floor() as i32 {
                    for b in block::collision_boxes(get(x, y, z)) {
                        let (bx, by, bz) = (x as f32, y as f32, z as f32);
                        if min.x + eps < bx + b[3] && max.x - eps > bx + b[0] && min.y + eps < by + b[4] && max.y - eps > by + b[1] && min.z + eps < bz + b[5] && max.z - eps > bz + b[2] {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    /// Standing in a ladder's cell.
    pub fn on_ladder(&self, get: &dyn Fn(i32, i32, i32) -> BlockId) -> bool {
        let (x, z) = (self.pos.x.floor() as i32, self.pos.z.floor() as i32);
        [0.1, 1.0].iter().any(|dy| block::is_ladder(get(x, (self.pos.y + dy).floor() as i32, z)))
    }

    /// Moves along one axis, stopping flush against the first solid block. True if blocked.
    fn sweep(&mut self, axis: usize, delta: f32, get: &dyn Fn(i32, i32, i32) -> BlockId) -> bool {
        if delta == 0.0 {
            return false;
        }
        let mut p = self.pos;
        p[axis] += delta;
        if !self.collides(p, get) {
            self.pos = p;
            return false;
        }
        // Binary search the largest free fraction of the move.
        let (mut lo, mut hi) = (0.0f32, 1.0f32);
        for _ in 0..12 {
            let mid = (lo + hi) * 0.5;
            let mut q = self.pos;
            q[axis] += delta * mid;
            if self.collides(q, get) { hi = mid } else { lo = mid }
        }
        self.pos[axis] += delta * lo;
        true
    }

    /// Advances the body by `dt` seconds. Returns fall damage taken on landing, if any.
    pub fn step(&mut self, dt: f32, input: &Input, get: &dyn Fn(i32, i32, i32) -> BlockId) -> f32 {
        let mut damage = 0.0;
        let mut left = dt.min(0.25);
        while left > 0.0 {
            let h = left.min(1.0 / 60.0);
            left -= h;
            damage += self.substep(h, input, get);
        }
        damage
    }

    fn substep(&mut self, dt: f32, input: &Input, get: &dyn Fn(i32, i32, i32) -> BlockId) -> f32 {
        let at = |y: f32| get(self.pos.x.floor() as i32, (self.pos.y + y).floor() as i32, self.pos.z.floor() as i32);
        let (feet, mid, head) = (at(0.1), at(0.9), at(EYE));
        self.in_water = block::is_liquid(feet) || block::is_liquid(mid);
        self.head_in_water = block::is_liquid(head);
        self.in_lava = feet == block::LAVA || mid == block::LAVA;

        let accel = if self.on_ground || input.flying { 14.0 } else if self.in_water { 6.0 } else { 3.0 };
        let k = (accel * dt).min(1.0);
        self.vel.x += (input.wish.x - self.vel.x) * k;
        self.vel.z += (input.wish.z - self.vel.z) * k;

        let ladder = !input.flying && self.on_ladder(get);
        if ladder {
            self.fall_from = None;
            self.vel.y = if input.jump || input.wish.length_squared() > 0.1 {
                2.8
            } else if input.descend {
                0.0
            } else {
                (self.vel.y - GRAVITY * dt).max(-2.5)
            };
        } else if input.flying {
            let target = if input.jump { FLY * 0.8 } else if input.descend { -FLY * 0.8 } else { 0.0 };
            self.vel.y += (target - self.vel.y) * (10.0 * dt).min(1.0);
            self.fall_from = None;
        } else if self.in_water {
            self.vel.y -= GRAVITY * 0.25 * dt;
            self.vel.y *= 1.0 - (2.5 * dt).min(1.0);
            if input.jump {
                self.vel.y = self.vel.y.max(3.2);
            }
            self.vel.y = self.vel.y.max(-4.0);
            self.fall_from = None;
        } else {
            self.vel.y -= GRAVITY * dt;
            self.vel.y = self.vel.y.max(-60.0);
            if input.jump && self.on_ground {
                self.vel.y = JUMP;
            }
        }

        let was_ground = self.on_ground;
        let blocked_y = self.sweep(1, self.vel.y * dt, get);
        let _ = ladder;
        let falling = self.vel.y < 0.0;
        if blocked_y {
            self.vel.y = 0.0;
        }
        self.on_ground = blocked_y && falling;
        if self.sweep(0, self.vel.x * dt, get) {
            self.vel.x = 0.0;
        }
        if self.sweep(2, self.vel.z * dt, get) {
            self.vel.z = 0.0;
        }

        let mut damage = 0.0;
        if !self.on_ground && !input.flying && !self.in_water {
            let top = self.fall_from.get_or_insert(self.pos.y);
            *top = top.max(self.pos.y);
        }
        if self.on_ground && !was_ground {
            if let Some(top) = self.fall_from.take() {
                damage = (top - self.pos.y - 3.0).max(0.0);
            }
        }
        damage
    }

    pub fn eye(&self) -> Vec3 {
        self.pos + Vec3::Y * EYE
    }

    /// True if a block at `b` would overlap the body.
    pub fn overlaps_block(&self, b: IVec3) -> bool {
        let min = self.pos - Vec3::new(HALF_W, 0.0, HALF_W);
        let max = self.pos + Vec3::new(HALF_W, HEIGHT, HALF_W);
        let bmin = b.as_vec3();
        let bmax = bmin + Vec3::ONE;
        min.x < bmax.x && max.x > bmin.x && min.y < bmax.y && max.y > bmin.y && min.z < bmax.z && max.z > bmin.z
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub pos: IVec3,
    pub normal: IVec3,
    pub block: BlockId,
    pub dist: f32,
}

/// Walks the grid from `origin` along `dir` and returns the first targetable block.
pub fn raycast(origin: Vec3, dir: Vec3, max: f32, get: &dyn Fn(i32, i32, i32) -> BlockId) -> Option<Hit> {
    raycast_with(origin, dir, max, get, false)
}

/// Like `raycast`, but fluid sources count as targets too (for buckets).
pub fn raycast_fluids(origin: Vec3, dir: Vec3, max: f32, get: &dyn Fn(i32, i32, i32) -> BlockId) -> Option<Hit> {
    raycast_with(origin, dir, max, get, true)
}

fn raycast_with(origin: Vec3, dir: Vec3, max: f32, get: &dyn Fn(i32, i32, i32) -> BlockId, fluids: bool) -> Option<Hit> {
    let dir = dir.normalize_or_zero();
    if dir == Vec3::ZERO {
        return None;
    }
    let mut cell = origin.floor().as_ivec3();
    let step = IVec3::new(dir.x.signum() as i32, dir.y.signum() as i32, dir.z.signum() as i32);
    let inv = Vec3::new(1.0 / dir.x.abs().max(1e-9), 1.0 / dir.y.abs().max(1e-9), 1.0 / dir.z.abs().max(1e-9));
    let frac = origin - origin.floor();
    let mut t_max = Vec3::new(
        if dir.x > 0.0 { 1.0 - frac.x } else { frac.x } * inv.x,
        if dir.y > 0.0 { 1.0 - frac.y } else { frac.y } * inv.y,
        if dir.z > 0.0 { 1.0 - frac.z } else { frac.z } * inv.z,
    );
    let mut normal = IVec3::ZERO;
    let mut t = 0.0;
    while t <= max {
        let b = get(cell.x, cell.y, cell.z);
        if b != block::AIR && (!block::is_liquid(b) || (fluids && block::is_source(b))) {
            return Some(Hit { pos: cell, normal, block: b, dist: t });
        }
        if t_max.x < t_max.y && t_max.x < t_max.z {
            t = t_max.x;
            t_max.x += inv.x;
            cell.x += step.x;
            normal = IVec3::new(-step.x, 0, 0);
        } else if t_max.y < t_max.z {
            t = t_max.y;
            t_max.y += inv.y;
            cell.y += step.y;
            normal = IVec3::new(0, -step.y, 0);
        } else {
            t = t_max.z;
            t_max.z += inv.z;
            cell.z += step.z;
            normal = IVec3::new(0, 0, -step.z);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn floor(_: i32, y: i32, _: i32) -> BlockId {
        if y < 10 { block::STONE } else { block::AIR }
    }

    #[test]
    fn falls_and_lands_on_the_floor() {
        let mut b = Body::new(Vec3::new(0.5, 14.0, 0.5));
        let input = Input { wish: Vec3::ZERO, jump: false, descend: false, flying: false };
        let mut dmg = 0.0;
        for _ in 0..120 {
            dmg += b.step(1.0 / 60.0, &input, &floor);
        }
        assert!((b.pos.y - 10.0).abs() < 0.01, "{}", b.pos.y);
        assert!(b.on_ground);
        assert!(dmg > 0.5 && dmg < 1.5, "4 block fall hurts a little: {dmg}");
    }

    #[test]
    fn ray_hits_the_floor_from_above() {
        let hit = raycast(Vec3::new(0.5, 12.5, 0.5), Vec3::new(0.0, -1.0, 0.0), 8.0, &floor).unwrap();
        assert_eq!(hit.pos, IVec3::new(0, 9, 0));
        assert_eq!(hit.normal, IVec3::Y);
    }
}
