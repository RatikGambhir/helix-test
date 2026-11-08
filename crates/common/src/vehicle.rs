//! Vehicles: minecarts and trains that follow rails, boats on water and cars on land. The
//! driver's client runs this physics and tells the server where the vehicle went.

use crate::block::{self, BlockId, DIRS};
use glam::{IVec3, Vec3};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum VehicleKind {
    Minecart,
    /// An engine pulling three passenger cars.
    Train,
    Boat,
    Car,
}

impl VehicleKind {
    pub fn item(self) -> BlockId {
        match self {
            VehicleKind::Minecart => block::MINECART,
            VehicleKind::Train => block::TRAIN,
            VehicleKind::Boat => block::BOAT,
            VehicleKind::Car => block::CAR,
        }
    }

    pub fn from_item(id: BlockId) -> Option<VehicleKind> {
        Some(match id {
            block::MINECART => VehicleKind::Minecart,
            block::TRAIN => VehicleKind::Train,
            block::BOAT => VehicleKind::Boat,
            block::CAR => VehicleKind::Car,
            _ => return None,
        })
    }

    pub fn on_rails(self) -> bool {
        matches!(self, VehicleKind::Minecart | VehicleKind::Train)
    }

    /// Seats: 0 drives. A train's seats 1..=3 are in its cars.
    pub fn seats(self) -> u8 {
        match self {
            VehicleKind::Minecart => 1,
            VehicleKind::Train => 4,
            VehicleKind::Boat | VehicleKind::Car => 2,
        }
    }

    pub fn cars(self) -> usize {
        if self == VehicleKind::Train { 3 } else { 0 }
    }

    /// Half the footprint (x, z) and the height, for clicking on it.
    pub fn half_size(self) -> Vec3 {
        match self {
            VehicleKind::Minecart => Vec3::new(0.5, 0.7, 0.7),
            VehicleKind::Train => Vec3::new(0.65, 1.8, 1.2),
            VehicleKind::Boat => Vec3::new(0.65, 0.6, 1.2),
            VehicleKind::Car => Vec3::new(0.9, 1.5, 1.75),
        }
    }

    /// Where a seat sits relative to the vehicle (or, in a train, to its car), unrotated.
    pub fn seat_offset(self, seat: u8) -> Vec3 {
        match (self, seat) {
            (VehicleKind::Minecart, _) => Vec3::new(0.0, 0.25, 0.0),
            (VehicleKind::Train, 0) => Vec3::new(0.0, 0.55, 0.5),
            (VehicleKind::Train, _) => Vec3::new(0.0, 0.3, 0.0),
            (VehicleKind::Boat, 0) => Vec3::new(0.0, 0.1, 0.3),
            (VehicleKind::Boat, _) => Vec3::new(0.0, 0.1, -0.5),
            (VehicleKind::Car, 0) => Vec3::new(-0.4, 0.25, 0.1),
            (VehicleKind::Car, _) => Vec3::new(0.4, 0.25, 0.1),
        }
    }
}

/// A vehicle as everyone sees it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VehicleState {
    pub id: u32,
    pub kind: VehicleKind,
    pub pos: [f32; 3],
    pub yaw: f32,
    pub speed: f32,
    /// Cars pulled behind (trains): position and yaw.
    pub cars: Vec<[f32; 4]>,
    /// (seat, player) pairs.
    pub riders: Vec<(u8, u32)>,
}

/// Forward, for a yaw (the same convention as players: yaw 0 faces -z).
pub fn forward(yaw: f32) -> Vec3 {
    Vec3::new(yaw.sin(), 0.0, -yaw.cos())
}

pub fn yaw_of(d: Vec3) -> f32 {
    d.x.atan2(-d.z)
}

/// Where seat `seat` is in the world.
pub fn seat_pos(kind: VehicleKind, pos: Vec3, yaw: f32, cars: &[[f32; 4]], seat: u8) -> Vec3 {
    let (base, yaw) = match (kind, seat) {
        (VehicleKind::Train, s) if s > 0 => match cars.get(s as usize - 1) {
            Some(c) => (Vec3::new(c[0], c[1], c[2]), c[3]),
            None => (pos, yaw),
        },
        _ => (pos, yaw),
    };
    let o = kind.seat_offset(seat);
    let (f, r) = (forward(yaw), forward(yaw + std::f32::consts::FRAC_PI_2));
    base + r * o.x + Vec3::Y * o.y + f * o.z
}

pub type Get<'a> = &'a dyn Fn(i32, i32, i32) -> BlockId;

// ---------------------------------------------------------------------------------------------
// Rails

/// Where a cart is on the track: the rail cell, the side it came in by, the side it leaves by,
/// and how far across the cell it is (0..1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Track {
    pub cell: IVec3,
    pub from: u8,
    pub to: u8,
    pub t: f32,
}

fn opposite(d: u8) -> u8 {
    (d + 2) & 3
}

fn dir_vec(d: u8) -> Vec3 {
    let (x, z) = DIRS[d as usize];
    Vec3::new(x as f32, 0.0, z as f32)
}

impl Track {
    /// Puts a cart on the rail at `cell`, heading the way closest to `yaw`.
    pub fn start(cell: IVec3, yaw: f32, get: Get) -> Option<Track> {
        let (exits, _) = block::rail_exits(get(cell.x, cell.y, cell.z))?;
        let f = forward(yaw);
        let (a, b) = (exits[0], exits[1]);
        let to = if dir_vec(a).dot(f) >= dir_vec(b).dot(f) { a } else { b };
        Some(Track { cell, from: if to == a { b } else { a }, to, t: 0.5 })
    }

    pub fn reversed(self) -> Track {
        Track { from: self.to, to: self.from, t: 1.0 - self.t, ..self }
    }

    /// The point on the track, and the direction of travel.
    pub fn pose(&self, get: Get) -> (Vec3, Vec3) {
        let c = self.cell.as_vec3() + Vec3::new(0.5, 0.0, 0.5);
        let (a, b) = (c + dir_vec(self.from) * 0.5, c + dir_vec(self.to) * 0.5);
        let mut p = if self.t < 0.5 { a.lerp(c, self.t * 2.0) } else { c.lerp(b, (self.t - 0.5) * 2.0) };
        let dir = if self.t < 0.5 { c - a } else { b - c }.normalize_or_zero();
        let up = block::rail_exits(get(self.cell.x, self.cell.y, self.cell.z)).and_then(|e| e.1);
        let rise = match up {
            Some(u) if u == self.to => self.t,
            Some(u) if u == self.from => 1.0 - self.t,
            _ => 0.0,
        };
        p.y = self.cell.y as f32 + 0.0625 + rise;
        let dir = match up {
            Some(u) if u == self.to => (dir + Vec3::Y).normalize(),
            Some(u) if u == self.from => (dir - Vec3::Y).normalize(),
            _ => dir,
        };
        (p, dir)
    }

    /// Slope under the cart: +1 climbing, -1 descending, 0 flat.
    pub fn slope(&self, get: Get) -> f32 {
        match block::rail_exits(get(self.cell.x, self.cell.y, self.cell.z)).and_then(|e| e.1) {
            Some(u) if u == self.to => 1.0,
            Some(u) if u == self.from => -1.0,
            _ => 0.0,
        }
    }

    /// The rail block under the cart.
    pub fn rail(&self, get: Get) -> BlockId {
        get(self.cell.x, self.cell.y, self.cell.z)
    }

    /// Moves `dist` along the track. Returns how far it really went (less at a dead end).
    pub fn advance(&mut self, mut dist: f32, get: Get) -> f32 {
        let mut moved = 0.0;
        while dist > 0.0 {
            let room = 1.0 - self.t;
            if dist < room {
                self.t += dist;
                return moved + dist;
            }
            dist -= room;
            moved += room;
            self.t = 1.0;
            let Some(next) = self.next(get) else { return moved };
            *self = next;
        }
        moved
    }

    /// The track in the next cell, if the rails continue.
    fn next(&self, get: Get) -> Option<Track> {
        let up = block::rail_exits(get(self.cell.x, self.cell.y, self.cell.z)).and_then(|e| e.1);
        let (dx, dz) = DIRS[self.to as usize];
        let mut n = self.cell + IVec3::new(dx, 0, dz);
        if up == Some(self.to) {
            n.y += 1;
        }
        let entry = opposite(self.to);
        let fits = |c: IVec3| -> Option<Track> {
            let (exits, _) = block::rail_exits(get(c.x, c.y, c.z))?;
            let other = if exits[0] == entry {
                exits[1]
            } else if exits[1] == entry {
                exits[0]
            } else {
                return None;
            };
            Some(Track { cell: c, from: entry, to: other, t: 0.0 })
        };
        if let Some(t) = fits(n) {
            return Some(t);
        }
        // A slope going down from here: its high side faces us, one block lower.
        let below = n - IVec3::Y;
        match block::rail_exits(get(below.x, below.y, below.z)) {
            Some((_, Some(u))) if u == entry => fits(below),
            _ => None,
        }
    }
}

/// Positions and yaws of `n` cars spaced `gap` apart, behind a cart at `track`.
pub fn cars_behind(track: Track, n: usize, gap: f32, get: Get) -> Vec<[f32; 4]> {
    let mut back = track.reversed();
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        back.advance(gap, get);
        let (p, d) = back.pose(get);
        let yaw = yaw_of(-d.with_y(0.0));
        out.push([p.x, p.y, p.z, yaw]);
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Driving

#[derive(Clone, Copy, Debug, Default)]
pub struct Controls {
    /// -1 back/brake .. 1 forward.
    pub throttle: f32,
    /// -1 left .. 1 right.
    pub steer: f32,
}

/// A vehicle being driven: its physics state.
#[derive(Clone, Debug)]
pub struct Drive {
    pub kind: VehicleKind,
    pub pos: Vec3,
    pub yaw: f32,
    pub speed: f32,
    pub vel_y: f32,
    pub track: Option<Track>,
}

pub const CAR_GAP: f32 = 2.4;

impl Drive {
    pub fn new(kind: VehicleKind, pos: Vec3, yaw: f32, get: Get) -> Drive {
        let mut d = Drive { kind, pos, yaw, speed: 0.0, vel_y: 0.0, track: None };
        if kind.on_rails() {
            let cell = pos.floor().as_ivec3();
            d.track = [cell, cell - IVec3::Y].into_iter().find_map(|c| Track::start(c, yaw, get));
            if let Some(t) = d.track {
                let (p, dir) = t.pose(get);
                d.pos = p;
                d.yaw = yaw_of(dir.with_y(0.0));
            }
        }
        d
    }

    /// Cars behind a train, for drawing and riding.
    pub fn cars(&self, get: Get) -> Vec<[f32; 4]> {
        match self.track {
            Some(t) if self.kind.cars() > 0 => cars_behind(t, self.kind.cars(), CAR_GAP, get),
            _ => Vec::new(),
        }
    }

    pub fn step(&mut self, dt: f32, c: Controls, get: Get) {
        let mut left = dt.min(0.25);
        while left > 0.0 {
            let h = left.min(1.0 / 60.0);
            left -= h;
            match self.kind {
                VehicleKind::Minecart | VehicleKind::Train => self.step_rails(h, c, get),
                VehicleKind::Boat => self.step_boat(h, c, get),
                VehicleKind::Car => self.step_car(h, c, get),
            }
        }
    }

    fn step_rails(&mut self, dt: f32, c: Controls, get: Get) {
        let Some(mut track) = self.track else { return };
        let engine = self.kind == VehicleKind::Train;
        let (push, top) = if engine { (5.0, 16.0) } else { (2.5, 6.0) };
        // Throttle: forward speeds up; back brakes, then reverses once stopped.
        if c.throttle > 0.0 {
            if self.speed < top {
                self.speed = (self.speed + push * c.throttle * dt).min(top);
            }
        } else if c.throttle < 0.0 {
            if self.speed > 0.05 {
                self.speed = (self.speed - 8.0 * dt).max(0.0);
            } else {
                track = track.reversed();
                self.speed = 1.0;
            }
        }
        let rail = track.rail(get);
        if block::is_powered_rail(rail) {
            if matches!(rail, block::POWERED_RAIL_ON | block::POWERED_RAIL_EW_ON) {
                self.speed = (self.speed + 14.0 * dt).min(12.0f32.max(if engine { top } else { 0.0 }));
                if self.speed < 0.5 {
                    self.speed = 2.0;
                }
            } else {
                self.speed = (self.speed - 12.0 * dt).max(0.0);
            }
        }
        self.speed -= track.slope(get) * 7.0 * dt;
        self.speed -= (0.25 + self.speed * 0.02) * dt;
        if self.speed < 0.0 {
            // Rolling back down a slope.
            track = track.reversed();
            self.speed = -self.speed;
        }
        let want = self.speed * dt;
        let went = track.advance(want, get);
        if went + 1e-4 < want {
            self.speed = 0.0;
        }
        let (p, dir) = track.pose(get);
        self.pos = p;
        let flat = dir.with_y(0.0);
        if flat.length_squared() > 1e-4 {
            self.yaw = yaw_of(flat);
        }
        self.track = Some(track);
    }

    fn step_boat(&mut self, dt: f32, c: Controls, get: Get) {
        let at = |p: Vec3| get(p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
        let water_here = block::is_liquid(at(self.pos - Vec3::Y * 0.2)) || block::is_liquid(at(self.pos));
        if water_here {
            self.speed += c.throttle * 5.0 * dt;
            self.speed = self.speed.clamp(-2.5, 8.0);
            self.speed -= self.speed * 0.6 * dt;
            // Float on top of the water.
            let mut top = self.pos.y.floor();
            while block::is_liquid(at(Vec3::new(self.pos.x, top + 0.5, self.pos.z))) {
                top += 1.0;
            }
            let target = top - 0.15;
            self.vel_y = (target - self.pos.y) * 4.0;
        } else {
            self.speed -= self.speed * 6.0 * dt;
            self.vel_y -= 25.0 * dt;
        }
        self.yaw += c.steer * 1.8 * dt * (0.4 + (self.speed.abs() / 8.0).min(1.0));
        self.move_with_collision(dt, get, 0.0);
    }

    fn step_car(&mut self, dt: f32, c: Controls, get: Get) {
        let grounded = self.on_ground(get);
        if grounded {
            if c.throttle > 0.0 {
                self.speed += c.throttle * if self.speed < 0.0 { 14.0 } else { 7.0 } * dt;
            } else if c.throttle < 0.0 {
                self.speed += c.throttle * if self.speed > 0.0 { 14.0 } else { 4.0 } * dt;
            } else {
                self.speed -= self.speed.signum() * (1.5 * dt).min(self.speed.abs());
            }
            self.speed = self.speed.clamp(-6.0, 22.0);
            let grip = (self.speed / 6.0).clamp(-1.0, 1.0);
            self.yaw += c.steer * 1.6 * grip * dt;
        }
        let at = |p: Vec3| get(p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
        if block::is_liquid(at(self.pos + Vec3::Y * 0.3)) {
            self.speed -= self.speed * 3.0 * dt;
        }
        self.vel_y -= 25.0 * dt;
        self.move_with_collision(dt, get, 1.0);
    }

    fn solid_at(get: Get, p: Vec3) -> bool {
        let b = get(p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
        let f = p - p.floor();
        block::collision_boxes(b).iter().any(|bx| f.x >= bx[0] && f.x <= bx[3] && f.y >= bx[1] && f.y <= bx[4] && f.z >= bx[2] && f.z <= bx[5])
    }

    /// Corners of the footprint, at height `y` above the bottom.
    fn corners(&self, pos: Vec3, y: f32) -> [Vec3; 4] {
        let h = self.kind.half_size();
        let (f, r) = (forward(self.yaw), forward(self.yaw + std::f32::consts::FRAC_PI_2));
        let (fx, rx) = (f * (h.z - 0.05), r * (h.x - 0.05));
        let b = pos + Vec3::Y * y;
        [b + fx + rx, b + fx - rx, b - fx + rx, b - fx - rx]
    }

    fn on_ground(&self, get: Get) -> bool {
        self.corners(self.pos, -0.06).iter().any(|&p| Self::solid_at(get, p))
    }

    /// Moves forward by the speed, climbing up to `step` blocks, and falls with `vel_y`.
    fn move_with_collision(&mut self, dt: f32, get: Get, step: f32) {
        let next = self.pos + forward(self.yaw) * self.speed * dt;
        let blocked = |p: Vec3, s: &Drive| [0.3, 0.9].iter().any(|&y| s.corners(p, y).iter().any(|&c| Self::solid_at(get, c)));
        if !blocked(next, self) {
            self.pos = next;
        } else if step > 0.0 && !blocked(next + Vec3::Y * step, self) {
            self.pos = next + Vec3::Y * step;
            self.vel_y = 0.0;
        } else {
            self.speed *= -0.2;
        }
        let dy = self.vel_y * dt;
        let moved = self.pos + Vec3::Y * dy;
        if dy < 0.0 && self.corners(moved, 0.0).iter().any(|&c| Self::solid_at(get, c)) {
            // Land on top of the block below.
            self.pos.y = (moved.y + 1.0).floor().max(moved.y);
            while self.corners(self.pos, 0.001).iter().any(|&c| Self::solid_at(get, c)) && self.pos.y < moved.y + 1.0 {
                self.pos.y += 0.0625;
            }
            self.vel_y = 0.0;
        } else if dy > 0.0 && self.corners(moved, 1.0).iter().any(|&c| Self::solid_at(get, c)) {
            self.vel_y = 0.0;
        } else {
            self.pos = moved;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn world(blocks: &HashMap<(i32, i32, i32), BlockId>) -> impl Fn(i32, i32, i32) -> BlockId + '_ {
        move |x, y, z| match blocks.get(&(x, y, z)) {
            Some(b) => *b,
            None if y < 10 => block::STONE,
            None => block::AIR,
        }
    }

    #[test]
    fn a_cart_follows_a_curve_and_climbs_a_slope() {
        let mut b = HashMap::new();
        // East along z=0 from x=0..5, a curve at x=5 turning south, south to z=5, then a slope up.
        for x in 0..5 {
            b.insert((x, 10, 0), block::RAIL_EW);
        }
        b.insert((5, 10, 0), block::RAIL_SW);
        for z in 1..5 {
            b.insert((5, 10, z), block::RAIL);
        }
        b.insert((5, 10, 5), block::rail_up(2));
        b.insert((5, 11, 6), block::RAIL);
        b.insert((5, 11, 7), block::RAIL);
        let get = world(&b);
        let mut d = Drive::new(VehicleKind::Train, Vec3::new(0.5, 10.1, 0.5), std::f32::consts::FRAC_PI_2, &get);
        assert!(d.track.is_some(), "the train sits on the rail");
        for _ in 0..600 {
            d.step(1.0 / 60.0, Controls { throttle: 1.0, steer: 0.0 }, &get);
        }
        let cell = d.track.unwrap().cell;
        assert!(cell.y == 11 && cell.z >= 6, "went round the bend and up the slope: {cell:?}");
        let cars = d.cars(&get);
        assert_eq!(cars.len(), 3);
        assert!(cars.iter().all(|c| c[1] >= 10.0), "cars stay on the track");
    }

    #[test]
    fn a_car_drives_and_climbs_one_block() {
        let mut b = HashMap::new();
        for x in -3..3 {
            for z in -30..4 {
                b.insert((x, 10, z), block::AIR);
            }
        }
        for x in -3..3 {
            b.insert((x, 10, -15), block::STONE);
            for z in -90..-15 {
                b.insert((x, 10, z), block::STONE);
            }
        }
        let get = world(&b);
        let mut d = Drive::new(VehicleKind::Car, Vec3::new(0.0, 10.0, 0.0), 0.0, &get);
        for _ in 0..240 {
            d.step(1.0 / 60.0, Controls { throttle: 1.0, steer: 0.0 }, &get);
        }
        assert!(d.pos.z < -16.0, "drove forward: {}", d.pos);
        assert!((d.pos.y - 11.0).abs() < 0.2, "and up the kerb: {}", d.pos);
    }
}
