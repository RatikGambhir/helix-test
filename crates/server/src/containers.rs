//! Chests and furnaces: their contents live on the server so everyone sees the same thing.

use crate::World;
use glam::Vec3;
use runecraft_common::block::{self, Stack};
use runecraft_common::craft::{SMELT_TIME, smelt};
use runecraft_common::protocol::{ContainerKind, ToClient};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Container {
    pub kind: ContainerKind,
    pub slots: Vec<Option<Stack>>,
    pub burn: f32,
    pub burn_max: f32,
    pub progress: f32,
    #[serde(skip)]
    pub since_sync: f32,
}

impl Container {
    fn new(kind: ContainerKind) -> Container {
        let n = if kind == ContainerKind::Chest { 27 } else { 3 };
        Container { kind, slots: vec![None; n], burn: 0.0, burn_max: 0.0, progress: 0.0, since_sync: 0.0 }
    }
    fn message(&self, pos: [i32; 3]) -> ToClient {
        let burn = if self.burn_max > 0.0 { (self.burn / self.burn_max).clamp(0.0, 1.0) } else { 0.0 };
        ToClient::Container { pos, kind: self.kind, slots: self.slots.clone(), burn, progress: self.progress / SMELT_TIME }
    }
}

impl World {
    pub(crate) fn open_container(&mut self, conn: u32, pos: [i32; 3]) {
        let kind = match self.get(pos[0], pos[1], pos[2]) {
            block::CHEST => ContainerKind::Chest,
            block::FURNACE | block::FURNACE_LIT => ContainerKind::Furnace,
            _ => return,
        };
        let c = self.containers.entry(pos).or_insert_with(|| Container::new(kind));
        let msg = c.message(pos);
        if let Some(p) = self.players.get_mut(&conn) {
            p.open = Some(pos);
            p.send(&msg);
        }
    }

    pub(crate) fn set_container_slot(&mut self, pos: [i32; 3], slot: u8, stack: Option<Stack>) {
        let Some(c) = self.containers.get_mut(&pos) else { return };
        if let Some(s) = c.slots.get_mut(slot as usize) {
            *s = stack.filter(|s| s.n > 0);
            self.dirty = true;
            self.sync_container(pos);
        }
    }

    fn sync_container(&self, pos: [i32; 3]) {
        let Some(c) = self.containers.get(&pos) else { return };
        let msg = c.message(pos);
        for p in self.players.values() {
            if p.open == Some(pos) {
                p.send(&msg);
            }
        }
    }

    pub(crate) fn spill_container(&mut self, pos: [i32; 3]) {
        let Some(c) = self.containers.remove(&pos) else { return };
        let at = Vec3::new(pos[0] as f32 + 0.5, pos[1] as f32 + 0.5, pos[2] as f32 + 0.5);
        for s in c.slots.into_iter().flatten() {
            let vel = Vec3::new(self.rand() - 0.5, 0.6, self.rand() - 0.5) * 4.0;
            self.spawn_item(s, at, vel, 0.5);
        }
        for p in self.players.values_mut() {
            if p.open == Some(pos) {
                p.open = None;
            }
        }
    }

    pub(crate) fn tick_containers(&mut self, dt: f32) {
        let furnaces: Vec<[i32; 3]> = self.containers.iter().filter(|(_, c)| c.kind == ContainerKind::Furnace).map(|(p, _)| *p).collect();
        for pos in furnaces {
            let (changed, lit) = {
                let c = self.containers.get_mut(&pos).unwrap();
                let before = (c.burn > 0.0, c.slots.clone());
                let out = c.slots[0].and_then(|s| smelt(s.id));
                let can = out.is_some_and(|o| c.slots[2].is_none_or(|s| s.id == o && s.n < block::max_stack(o)));
                if c.burn > 0.0 {
                    c.burn = (c.burn - dt).max(0.0);
                }
                if can
                    && c.burn <= 0.0
                    && let Some(f) = c.slots[1]
                {
                    let t = block::fuel_time(f.id);
                    if t > 0.0 {
                        c.burn = t;
                        c.burn_max = t;
                        c.slots[1] = if f.id == block::LAVA_BUCKET {
                            Some(Stack::new(block::BUCKET, 1))
                        } else if f.n > 1 {
                            Some(Stack { n: f.n - 1, ..f })
                        } else {
                            None
                        };
                    }
                }
                if can && c.burn > 0.0 {
                    c.progress += dt;
                    if c.progress >= SMELT_TIME {
                        c.progress = 0.0;
                        let o = out.unwrap();
                        let input = c.slots[0].unwrap();
                        c.slots[0] = if input.n > 1 { Some(Stack { n: input.n - 1, ..input }) } else { None };
                        c.slots[2] = Some(match c.slots[2] {
                            Some(s) => Stack { n: s.n + 1, ..s },
                            None => Stack::new(o, 1),
                        });
                    }
                } else {
                    c.progress = 0.0;
                }
                c.since_sync += dt;
                let lit = c.burn > 0.0;
                let changed = before.0 != lit || before.1 != c.slots || (lit && c.since_sync > 0.25);
                if changed {
                    c.since_sync = 0.0;
                }
                (changed, lit)
            };
            if changed {
                self.dirty = true;
                self.sync_container(pos);
                if self.loaded(pos[0], pos[2]) {
                    let b = self.get(pos[0], pos[1], pos[2]);
                    let want = if lit { block::FURNACE_LIT } else { block::FURNACE };
                    if (b == block::FURNACE || b == block::FURNACE_LIT) && b != want {
                        // Swap the block without spilling: it stays a furnace.
                        self.set_block(pos, want);
                    }
                }
            }
        }
    }
}
