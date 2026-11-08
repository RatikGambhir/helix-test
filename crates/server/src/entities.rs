//! Creatures, dropped items and arrows: spawning, AI, physics, combat, drops and pickup.

use crate::World;
use glam::Vec3;
use runecraft_common::block::{self, BlockId, Stack};
use runecraft_common::chunk;
use runecraft_common::city::{self, Zone};
use runecraft_common::physics::{self, Body};
use runecraft_common::protocol::{EntityKind, EntityState, ToClient};

pub(crate) struct Entity {
    pub kind: EntityKind,
    pub body: Body,
    pub yaw: f32,
    pub pitch: f32,
    pub health: f32,
    wander: Vec3,
    timer: f32,
    hurt: f32,
    attack_cd: f32,
    flee: f32,
    pub item: Option<Stack>,
    age: f32,
    /// Who fired an arrow (a player's connection).
    owner: Option<u32>,
    from_player: bool,
    variant: u8,
    pickup_delay: f32,
}

impl Entity {
    fn new(kind: EntityKind, pos: Vec3) -> Entity {
        let health = match kind {
            EntityKind::Zombie | EntityKind::Skeleton | EntityKind::Citizen => 20.0,
            EntityKind::Spider => 16.0,
            EntityKind::Cow => 10.0,
            EntityKind::Chicken => 4.0,
            _ => 8.0,
        };
        Entity {
            kind,
            body: Body::new(pos),
            yaw: 0.0,
            pitch: 0.0,
            health,
            wander: Vec3::ZERO,
            timer: 0.0,
            hurt: 0.0,
            attack_cd: 1.0,
            flee: 0.0,
            item: None,
            age: 0.0,
            owner: None,
            from_player: false,
            variant: 0,
            pickup_delay: 0.0,
        }
    }
}

const CITIZEN_LINES: &[(&str, &[&str])] = &[
    ("New York", &["Times Square is right up the avenue. Can't miss the lights!", "Central Park has a lake. Perfect for a swim.", "Watch out for the cabs, they don't stop for anybody.", "The Empire tower is in midtown. Best view in the city."]),
    ("Tokyo", &["Konnichiwa! The scramble crossing is in the middle of town.", "Have you been up Tokyo Tower? Red and white, you'll see it.", "The cherry trees at the shrine are in bloom.", "The neon is best at night."]),
    ("Paris", &["Bonjour! The iron tower is just south of the arch.", "Have you tried a fresh baguette? Craft one from wheat!", "The rooftops here are all slate. Très chic."]),
    ("London", &["Cheers! The clock tower is down by the river.", "Fancy a ride on the big wheel?", "Mind the gap. And the red cars."]),
    ("Chicago", &["The tallest tower in town is in the Loop.", "Have you seen the bean in the park? Shiny!", "The L trains run right over the streets.", "Lake Michigan is east. Bring a boat. Or swim."]),
    ("Sydney", &["G'day! The opera house is on the harbour.", "Walk across the harbour bridge, it's a ripper.", "Beach is that way, mate."]),
    ("Dubai", &["Welcome! The tallest tower in the world is downtown.", "The desert starts right where the city ends.", "Mind the heat."]),
    ("Rome", &["Ciao! The Colosseum is in the old town.", "All roads lead to Rome. Even the ones you build.", "Gelato? I wish we had gelato."]),
];

impl World {
    fn add_entity(&mut self, e: Entity) -> u32 {
        let id = self.next_entity;
        self.next_entity += 1;
        self.entities.insert(id, e);
        id
    }

    pub(crate) fn spawn_item(&mut self, stack: Stack, pos: Vec3, vel: Vec3, delay: f32) {
        if stack.n == 0 || stack.id == block::AIR {
            return;
        }
        let mut e = Entity::new(EntityKind::Item, pos - Vec3::Y * 0.1);
        e.body.vel = vel;
        e.item = Some(stack);
        e.pickup_delay = delay;
        self.add_entity(e);
    }

    pub(crate) fn spawn_arrow(&mut self, pos: Vec3, vel: Vec3, owner: Option<u32>, from_player: bool) {
        let mut e = Entity::new(EntityKind::Arrow, pos);
        e.body.vel = vel.clamp_length_max(60.0);
        e.owner = owner;
        e.from_player = from_player;
        self.add_entity(e);
    }

    pub(crate) fn attack(&mut self, conn: u32, id: u32, damage: f32) {
        let Some(from) = self.players.get(&conn).map(|p| p.pos()) else { return };
        if self.entities.get(&id).is_some_and(|e| e.body.pos.distance(from) < 7.0) {
            self.hurt_entity(id, damage.clamp(0.0, 15.0), from);
        }
    }

    pub(crate) fn hurt_entity(&mut self, id: u32, damage: f32, from: Vec3) {
        let Some(e) = self.entities.get_mut(&id) else { return };
        if !e.kind.living() || e.hurt > 0.2 {
            return;
        }
        e.health -= damage;
        e.hurt = 0.45;
        e.flee = 5.0;
        let away = (e.body.pos - from).with_y(0.0).normalize_or_zero();
        e.body.vel += away * 6.0 + Vec3::Y * 4.5;
        if e.health > 0.0 {
            return;
        }
        let e = self.entities.remove(&id).unwrap();
        let mut drops: Vec<(BlockId, u8)> = Vec::new();
        let r = |w: &mut World, max: u8| (w.rand() * (max as f32 + 1.0)) as u8;
        match e.kind {
            EntityKind::Pig => drops.push((block::RAW_PORK, 1 + r(self, 2))),
            EntityKind::Cow => {
                drops.push((block::RAW_BEEF, 1 + r(self, 2)));
                drops.push((block::LEATHER, r(self, 2)));
            }
            EntityKind::Sheep => {
                drops.push((block::RAW_MUTTON, 1 + r(self, 1)));
                drops.push((block::WOOL_WHITE, 1));
            }
            EntityKind::Chicken => {
                drops.push((block::RAW_CHICKEN, 1));
                drops.push((block::FEATHER, r(self, 2)));
            }
            EntityKind::Zombie => drops.push((block::ROTTEN_FLESH, r(self, 2))),
            EntityKind::Skeleton => {
                drops.push((block::BONE, r(self, 2)));
                drops.push((block::ARROW, r(self, 2)));
            }
            EntityKind::Spider => drops.push((block::STRING, r(self, 2))),
            _ => {}
        }
        for (id, n) in drops {
            if n > 0 {
                let vel = Vec3::new(self.rand() - 0.5, 1.0, self.rand() - 0.5) * 3.0;
                self.spawn_item(Stack::new(id, n), e.body.pos + Vec3::Y * 0.5, vel, 0.5);
            }
        }
    }

    pub(crate) fn interact(&mut self, conn: u32, id: u32) {
        let Some(e) = self.entities.get(&id) else { return };
        if e.kind != EntityKind::Citizen {
            return;
        }
        let (pos, variant) = (e.body.pos, e.variant);
        let city = match city::zone(pos.x as i32, pos.z as i32) {
            Zone::Core(c) | Zone::Blend(c, _) | Zone::River(c) => c.name,
            Zone::None => "",
        };
        let lines = CITIZEN_LINES.iter().find(|(n, _)| *n == city).map_or(&["Nice day, isn't it?"][..], |(_, l)| *l);
        let line = lines[(self.rand() * lines.len() as f32) as usize % lines.len()];
        let names = ["Aiko", "Ben", "Chloé", "Dev", "Emma", "Femi", "Gus", "Hana", "Ivan", "Jules", "Kai", "Lena"];
        let name = names[variant as usize % names.len()];
        if let Some(p) = self.players.get(&conn) {
            p.send(&ToClient::Chat { from: Some(name.to_string()), text: line.to_string() });
        }
    }

    fn walkable_city(&self, x: i32, y: i32, z: i32) -> bool {
        matches!(self.get(x, y, z), block::SIDEWALK | block::GRAVEL | block::GRASS | block::ROAD_WHITE)
    }

    pub(crate) fn spawn_creatures(&mut self) {
        if !self.cfg.mobs {
            return;
        }
        let positions: Vec<Vec3> = self.players.values().map(|p| p.pos()).collect();
        let night = self.is_night();
        for at in positions {
            let near = |w: &World, f: fn(EntityKind) -> bool| w.entities.values().filter(|m| f(m.kind) && m.body.pos.distance(at) < 72.0).count();
            let passive = near(self, |k| matches!(k, EntityKind::Pig | EntityKind::Sheep | EntityKind::Cow | EntityKind::Chicken));
            let hostile = near(self, EntityKind::hostile);
            let citizens = near(self, |k| k == EntityKind::Citizen);
            let ang = self.rand() * std::f32::consts::TAU;
            let dist = 18.0 + self.rand() * 34.0;
            let x = (at.x + ang.cos() * dist).floor() as i32;
            let z = (at.z + ang.sin() * dist).floor() as i32;
            if !self.loaded(x, z) {
                continue;
            }
            let ((cx, lx), (cz, lz)) = (chunk::split(x), chunk::split(z));
            let top = self.columns[&(cx, cz)].col.height(lx, lz);
            if top < 1 {
                continue;
            }
            let ground = self.get(x, top, z);
            let zone = city::zone(x, z);
            let in_city = matches!(zone, Zone::Core(_));
            let roll = self.rand();
            let mut spawn: Option<(EntityKind, Vec3)> = None;
            let surface = Vec3::new(x as f32 + 0.5, top as f32 + 1.0, z as f32 + 0.5);
            if in_city && !night && citizens < 16 && self.walkable_city(x, top, z) {
                spawn = Some((EntityKind::Citizen, surface));
            } else if night && hostile < 10 && roll < 0.8 && block::is_solid(ground) {
                let kind = [EntityKind::Zombie, EntityKind::Skeleton, EntityKind::Spider][(self.rand() * 3.0) as usize % 3];
                spawn = Some((kind, surface));
            } else if !night && !in_city && passive < 10 && ground == block::GRASS {
                let kind = [EntityKind::Pig, EntityKind::Sheep, EntityKind::Cow, EntityKind::Chicken][(self.rand() * 4.0) as usize % 4];
                spawn = Some((kind, surface));
            } else if hostile < 6 && at.y < top as f32 - 6.0 {
                // Dark caves breed monsters even by day.
                let y = (at.y as i32 + (self.rand() * 16.0) as i32 - 8).clamp(4, top - 6);
                let free = |w: &World, y: i32| !block::is_solid(w.get(x, y, z)) && block::fluid(w.get(x, y, z)).is_none();
                if free(self, y) && free(self, y + 1) && block::is_solid(self.get(x, y - 1, z)) {
                    let kind = [EntityKind::Zombie, EntityKind::Skeleton, EntityKind::Spider][(self.rand() * 3.0) as usize % 3];
                    spawn = Some((kind, Vec3::new(x as f32 + 0.5, y as f32, z as f32 + 0.5)));
                }
            }
            if let Some((kind, pos)) = spawn {
                let mut e = Entity::new(kind, pos);
                e.yaw = ang;
                e.variant = (self.rand() * 64.0) as u8;
                self.add_entity(e);
            }
        }
    }

    fn point_solid(&self, p: Vec3) -> bool {
        let b = self.get(p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
        let f = p - p.floor();
        block::collision_boxes(b).iter().any(|bx| f.x >= bx[0] && f.x <= bx[3] && f.y >= bx[1] && f.y <= bx[4] && f.z >= bx[2] && f.z <= bx[5])
    }

    pub(crate) fn tick_entities(&mut self, dt: f32) {
        let players: Vec<(u32, Vec3)> = self.players.iter().map(|(id, p)| (*id, p.pos())).collect();
        let day = !self.is_night();
        let ids: Vec<u32> = self.entities.keys().copied().collect();
        let mut player_hits: Vec<(u32, f32, Vec3)> = Vec::new();
        let mut arrows: Vec<(Vec3, Vec3)> = Vec::new();
        let mut entity_hits: Vec<(u32, f32, Vec3)> = Vec::new();
        let mut gone = Vec::new();
        for id in ids {
            let Some(mut e) = self.entities.remove(&id) else { continue };
            e.age += dt;
            e.hurt = (e.hurt - dt).max(0.0);
            let nearest = players.iter().map(|(pid, p)| (*pid, *p, p.distance(e.body.pos))).min_by(|a, b| a.2.total_cmp(&b.2));
            let far = nearest.is_none_or(|n| n.2 > 100.0);
            let loaded = self.loaded(e.body.pos.x.floor() as i32, e.body.pos.z.floor() as i32);
            if far || !loaded || e.body.pos.y < -10.0 {
                gone.push(id);
                continue;
            }
            match e.kind {
                EntityKind::Item => {
                    e.pickup_delay -= dt;
                    if e.age > 300.0 {
                        gone.push(id);
                        continue;
                    }
                    self.step_point(&mut e, dt, 18.0);
                    if e.pickup_delay <= 0.0
                        && let Some((pid, _, d)) = players.iter().map(|(pid, p)| (*pid, *p, (*p + Vec3::Y * 0.6).distance(e.body.pos))).min_by(|a, b| a.2.total_cmp(&b.2))
                        && d < 1.8
                        && let Some(stack) = e.item
                    {
                        if let Some(p) = self.players.get(&pid) {
                            p.send(&ToClient::Pickup(stack));
                        }
                        gone.push(id);
                        continue;
                    }
                }
                EntityKind::Arrow => {
                    if e.age > 30.0 {
                        gone.push(id);
                        continue;
                    }
                    let steps = 4;
                    let h = dt / steps as f32;
                    let mut hit = false;
                    for _ in 0..steps {
                        e.body.vel.y -= 20.0 * h;
                        let next = e.body.pos + e.body.vel * h;
                        if self.point_solid(next) {
                            hit = true;
                            if e.from_player && self.rand() < 0.5 {
                                arrows.push((e.body.pos, Vec3::ZERO));
                            }
                            break;
                        }
                        e.body.pos = next;
                        let dmg = (e.body.vel.length() * 0.3).clamp(2.0, 9.0);
                        if let Some((oid, _)) = self.entities.iter().find(|(_, o)| o.kind.living() && (o.body.pos + Vec3::Y * 0.8).distance(e.body.pos) < 0.75) {
                            entity_hits.push((*oid, dmg, e.body.pos - e.body.vel.normalize_or_zero()));
                            hit = true;
                            break;
                        }
                        if !e.from_player
                            && let Some((pid, _)) = players.iter().find(|(_, p)| (*p + Vec3::Y * 0.9).distance(e.body.pos) < 0.8)
                        {
                            player_hits.push((*pid, dmg, e.body.pos - e.body.vel));
                            hit = true;
                            break;
                        }
                    }
                    let v = e.body.vel;
                    e.yaw = v.x.atan2(-v.z);
                    e.pitch = v.y.atan2(v.with_y(0.0).length());
                    if hit {
                        gone.push(id);
                        continue;
                    }
                }
                _ => {
                    let exposed = {
                        let p = e.body.pos.floor().as_ivec3();
                        let ((cx, lx), (cz, lz)) = (chunk::split(p.x), chunk::split(p.z));
                        self.columns.get(&(cx, cz)).is_none_or(|l| l.col.height(lx, lz) < p.y)
                    };
                    let burns = day && exposed && matches!(e.kind, EntityKind::Zombie | EntityKind::Skeleton);
                    if (burns && self.rand() < dt * 0.15) || (e.kind == EntityKind::Citizen && !day) {
                        gone.push(id);
                        continue;
                    }
                    let wish = self.think(&mut e, nearest, dt, &mut player_hits, &mut arrows);
                    if wish.length_squared() > 0.01 {
                        e.yaw = wish.z.atan2(wish.x);
                    }
                    let jump = {
                        let ahead = e.body.pos + wish.normalize_or_zero() * 0.7;
                        wish.length_squared() > 0.01
                            && (e.body.on_ground || e.body.in_water)
                            && block::is_solid(self.get(ahead.x.floor() as i32, e.body.pos.y.floor() as i32, ahead.z.floor() as i32))
                    } || (e.body.in_water && e.kind != EntityKind::Chicken);
                    let input = physics::Input { wish, jump, descend: false, flying: false };
                    let get = |x: i32, y: i32, z: i32| self.get(x, y, z);
                    let fall = e.body.step(dt, &input, &get);
                    if e.kind == EntityKind::Chicken && e.body.vel.y < -3.0 {
                        e.body.vel.y = -3.0;
                    }
                    if fall > 0.0 && e.kind != EntityKind::Chicken {
                        e.health -= fall;
                        if e.health <= 0.0 {
                            gone.push(id);
                            continue;
                        }
                    }
                    if e.body.in_lava {
                        e.health -= dt * 6.0;
                        if e.health <= 0.0 {
                            gone.push(id);
                            continue;
                        }
                    }
                }
            }
            self.entities.insert(id, e);
        }
        for id in gone {
            self.entities.remove(&id);
        }
        for (pos, _) in arrows.iter().filter(|a| a.1 == Vec3::ZERO) {
            self.spawn_item(Stack::new(block::ARROW, 1), *pos, Vec3::ZERO, 0.5);
        }
        for (pos, vel) in arrows.into_iter().filter(|a| a.1 != Vec3::ZERO) {
            self.spawn_arrow(pos, vel, None, false);
        }
        for (id, dmg, from) in entity_hits {
            self.hurt_entity(id, dmg, from);
        }
        for (pid, amount, from) in player_hits {
            if let Some(p) = self.players.get_mut(&pid)
                && p.hurt_cooldown <= 0.0
            {
                p.hurt_cooldown = 0.5;
                p.send(&ToClient::Damage { amount, from: from.to_array() });
            }
        }
        for p in self.players.values_mut() {
            p.hurt_cooldown -= dt;
        }
        // Merge item stacks lying together.
        if (self.time * 24000.0) as i32 % 20 == 0 {
            self.merge_items();
        }
        for p in self.players.values() {
            let at = p.pos();
            let list: Vec<EntityState> = self
                .entities
                .iter()
                .filter(|(_, m)| m.body.pos.distance(at) < 80.0)
                .map(|(id, m)| EntityState {
                    id: *id,
                    kind: m.kind,
                    pos: m.body.pos.to_array(),
                    yaw: m.yaw,
                    pitch: m.pitch,
                    hurt: m.hurt > 0.0,
                    item: match m.kind {
                        EntityKind::Item => m.item.map_or(0, |s| s.id),
                        _ => m.variant,
                    },
                })
                .collect();
            p.send(&ToClient::Entities(list));
        }
    }

    fn merge_items(&mut self) {
        let items: Vec<(u32, Vec3, Stack)> = self.entities.iter().filter_map(|(id, e)| e.item.map(|s| (*id, e.body.pos, s))).collect();
        let mut removed = std::collections::HashSet::new();
        for (i, (a, pa, sa)) in items.iter().enumerate() {
            if removed.contains(a) {
                continue;
            }
            for (b, pb, sb) in items.iter().skip(i + 1) {
                if removed.contains(b) || !sa.same(sb) || pa.distance(*pb) > 1.2 {
                    continue;
                }
                let total = self.entities[a].item.unwrap().n as u32 + sb.n as u32;
                if total > block::max_stack(sa.id) as u32 {
                    continue;
                }
                if let Some(e) = self.entities.get_mut(a) {
                    e.item = Some(Stack { n: total as u8, ..*sa });
                }
                removed.insert(*b);
            }
        }
        for id in removed {
            self.entities.remove(&id);
        }
    }

    /// Simple falling, sliding point physics for items.
    fn step_point(&self, e: &mut Entity, dt: f32, gravity: f32) {
        e.body.vel.y -= gravity * dt;
        let in_water = block::fluid(self.get(e.body.pos.x.floor() as i32, e.body.pos.y.floor() as i32, e.body.pos.z.floor() as i32)).is_some();
        if in_water {
            e.body.vel.y = e.body.vel.y.max(-1.0) + 12.0 * dt;
            e.body.vel *= 1.0 - (2.0 * dt).min(1.0);
        }
        for axis in [1usize, 0, 2] {
            let mut next = e.body.pos;
            next[axis] += e.body.vel[axis] * dt;
            if self.point_solid(next) {
                if axis == 1 && e.body.vel.y < 0.0 {
                    e.body.vel.x *= 0.6;
                    e.body.vel.z *= 0.6;
                }
                e.body.vel[axis] = 0.0;
            } else {
                e.body.pos = next;
            }
        }
    }

    /// Decides where a creature wants to go, and lets hostiles attack.
    fn think(&mut self, e: &mut Entity, nearest: Option<(u32, Vec3, f32)>, dt: f32, hits: &mut Vec<(u32, f32, Vec3)>, arrows: &mut Vec<(Vec3, Vec3)>) -> Vec3 {
        e.timer -= dt;
        e.flee = (e.flee - dt).max(0.0);
        e.attack_cd = (e.attack_cd - dt).max(0.0);
        let wander = |w: &mut World, e: &mut Entity, speed: f32| {
            if e.timer <= 0.0 {
                e.timer = 2.0 + w.rand() * 4.0;
                let a = w.rand() * std::f32::consts::TAU;
                e.wander = if w.rand() < 0.35 { Vec3::ZERO } else { Vec3::new(a.cos(), 0.0, a.sin()) * speed };
            }
            e.wander
        };
        match e.kind {
            EntityKind::Zombie | EntityKind::Spider => {
                let speed = if e.kind == EntityKind::Spider { 4.2 } else { 3.0 };
                if let Some((pid, p, d)) = nearest
                    && d < 26.0
                {
                    let dir = (p - e.body.pos).with_y(0.0).normalize_or_zero();
                    if d < 1.5 && (p.y - e.body.pos.y).abs() < 1.8 && e.attack_cd == 0.0 {
                        e.attack_cd = 1.0;
                        hits.push((pid, if e.kind == EntityKind::Spider { 2.0 } else { 3.0 }, e.body.pos));
                    }
                    if e.kind == EntityKind::Spider && d < 4.0 && e.body.on_ground {
                        e.body.vel.y = 6.5;
                    }
                    return dir * speed;
                }
                wander(self, e, 1.2)
            }
            EntityKind::Skeleton => {
                if let Some((_, p, d)) = nearest
                    && d < 24.0
                {
                    let to = p + Vec3::Y * 1.4 - (e.body.pos + Vec3::Y * 1.5);
                    let dir = to.with_y(0.0).normalize_or_zero();
                    e.yaw = dir.z.atan2(dir.x);
                    if d < 16.0 && e.attack_cd == 0.0 {
                        e.attack_cd = 2.2;
                        let speed = 24.0;
                        let flat = to.with_y(0.0).length();
                        let t = flat / speed;
                        let vel = to / t.max(0.05) + Vec3::Y * 0.5 * 20.0 * t;
                        let vel = vel.clamp_length_max(30.0);
                        arrows.push((e.body.pos + Vec3::Y * 1.5 + dir * 0.6, vel));
                    }
                    return if d > 12.0 { dir * 2.6 } else if d < 6.0 { -dir * 2.2 } else { Vec3::ZERO };
                }
                wander(self, e, 1.2)
            }
            EntityKind::Citizen => {
                if e.timer <= 0.0 || e.body.vel.with_y(0.0).length() < 0.2 && e.wander != Vec3::ZERO {
                    e.timer = 3.0 + self.rand() * 6.0;
                    let dirs = [Vec3::X, -Vec3::X, Vec3::Z, -Vec3::Z];
                    e.wander = if self.rand() < 0.2 { Vec3::ZERO } else { dirs[(self.rand() * 4.0) as usize % 4] * 1.5 };
                }
                // Stay on pavement: turn back before stepping onto the road or into a wall.
                let ahead = e.body.pos + e.wander.normalize_or_zero() * 0.8;
                let below = self.get(ahead.x.floor() as i32, e.body.pos.y.floor() as i32 - 1, ahead.z.floor() as i32);
                if e.wander != Vec3::ZERO && !matches!(below, block::SIDEWALK | block::GRAVEL | block::GRASS | block::ROAD_WHITE | block::MARBLE) {
                    e.wander = -e.wander;
                }
                e.wander
            }
            _ => {
                // Passive animals run when hurt, and wander otherwise.
                if e.flee > 0.0
                    && let Some((_, p, _)) = nearest
                {
                    return (e.body.pos - p).with_y(0.0).normalize_or_zero() * 3.6;
                }
                wander(self, e, 1.4)
            }
        }
    }
}
