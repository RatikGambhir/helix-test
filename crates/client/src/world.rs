//! The client's copy of the world: columns from the server, and which sections need meshing.

use crate::mesh::{Job, Mesher, Output, SectionKey};
use runecraft_common::block::{self, AIR, BlockId};
use runecraft_common::chunk::{self, CHUNK, Column, SECTIONS};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

pub struct World {
    pub columns: HashMap<(i32, i32), Arc<Column>>,
    /// Columns whose sections were queued for meshing (all neighbours present at the time).
    meshed: HashSet<(i32, i32)>,
    dirty: HashSet<SectionKey>,
    urgent: Vec<SectionKey>,
    versions: HashMap<SectionKey, u32>,
    pub mesher: Mesher,
    /// Finished meshes for the renderer to upload.
    pub ready: Vec<Output>,
    /// Sections the renderer should drop.
    pub removed: Vec<SectionKey>,
}

impl World {
    pub fn new() -> World {
        World {
            columns: HashMap::new(),
            meshed: HashSet::new(),
            dirty: HashSet::new(),
            urgent: Vec::new(),
            versions: HashMap::new(),
            mesher: Mesher::new(),
            ready: Vec::new(),
            removed: Vec::new(),
        }
    }

    pub fn clear(&mut self) {
        for &(cx, cz) in self.columns.keys() {
            for sy in 0..SECTIONS {
                self.removed.push((cx, sy, cz));
            }
        }
        self.columns.clear();
        self.meshed.clear();
        self.dirty.clear();
        self.urgent.clear();
    }

    #[inline]
    pub fn get(&self, x: i32, y: i32, z: i32) -> BlockId {
        let ((cx, lx), (cz, lz)) = (chunk::split(x), chunk::split(z));
        self.columns.get(&(cx, cz)).map_or(AIR, |c| c.get(lx, y, lz))
    }

    /// For collision: unloaded space is solid so nobody falls out of the world while it loads.
    pub fn get_solid(&self, x: i32, y: i32, z: i32) -> BlockId {
        let ((cx, lx), (cz, lz)) = (chunk::split(x), chunk::split(z));
        self.columns.get(&(cx, cz)).map_or(block::BEDROCK, |c| c.get(lx, y, lz))
    }

    pub fn loaded(&self, x: i32, z: i32) -> bool {
        let (cx, cz) = (x.div_euclid(CHUNK), z.div_euclid(CHUNK));
        self.columns.contains_key(&(cx, cz))
    }

    pub fn sky_exposed(&self, x: i32, y: i32, z: i32) -> bool {
        let ((cx, lx), (cz, lz)) = (chunk::split(x), chunk::split(z));
        self.columns.get(&(cx, cz)).is_none_or(|c| c.height(lx, lz) < y)
    }

    pub fn insert(&mut self, cx: i32, cz: i32, col: Column) {
        self.columns.insert((cx, cz), Arc::new(col));
        if self.meshed.contains(&(cx, cz)) {
            // A resend replaces it: remesh it and its neighbours' borders.
            for dz in -1..=1 {
                for dx in -1..=1 {
                    self.mark_column(cx + dx, cz + dz);
                }
            }
            return;
        }
        for dz in -1..=1 {
            for dx in -1..=1 {
                let k = (cx + dx, cz + dz);
                if !self.meshed.contains(&k) && self.has_neighbours(k.0, k.1) {
                    self.meshed.insert(k);
                    self.mark_column(k.0, k.1);
                }
            }
        }
    }

    fn mark_column(&mut self, cx: i32, cz: i32) {
        if !self.meshed.contains(&(cx, cz)) {
            return;
        }
        for sy in 0..SECTIONS {
            self.dirty.insert((cx, sy, cz));
        }
    }

    fn has_neighbours(&self, cx: i32, cz: i32) -> bool {
        (-1..=1).all(|dz| (-1..=1).all(|dx| self.columns.contains_key(&(cx + dx, cz + dz))))
    }

    pub fn remove(&mut self, cx: i32, cz: i32) {
        if self.columns.remove(&(cx, cz)).is_some() {
            for sy in 0..SECTIONS {
                self.removed.push((cx, sy, cz));
                self.dirty.remove(&(cx, sy, cz));
            }
            self.meshed.remove(&(cx, cz));
        }
    }

    pub fn set(&mut self, x: i32, y: i32, z: i32, b: BlockId) {
        let ((cx, lx), (cz, lz)) = (chunk::split(x), chunk::split(z));
        let Some(col) = self.columns.get_mut(&(cx, cz)) else { return };
        if col.get(lx, y, lz) == b {
            return;
        }
        Arc::make_mut(col).set(lx, y, lz, b);
        let sy = y.div_euclid(CHUNK);
        // The block's own section first, then everything its light could reach.
        self.urgent.push((cx, sy, cz));
        for dz in -1..=1 {
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let k = (cx + dx, sy + dy, cz + dz);
                    if (0..SECTIONS).contains(&k.1) && self.meshed.contains(&(k.0, k.2)) && (dx, dy, dz) != (0, 0, 0) {
                        self.dirty.insert(k);
                    }
                }
            }
        }
    }

    /// Sends the nearest dirty sections to the mesher, and collects finished meshes.
    pub fn pump(&mut self, near: (i32, i32, i32)) {
        while let Ok(out) = self.mesher.results.try_recv() {
            self.mesher.in_flight = self.mesher.in_flight.saturating_sub(1);
            if self.versions.get(&out.key).copied() == Some(out.version) && self.columns.contains_key(&(out.key.0, out.key.2)) {
                self.ready.push(out);
            }
        }
        let limit = self.mesher.workers * 6;
        let urgent: Vec<SectionKey> = std::mem::take(&mut self.urgent);
        for k in urgent {
            self.dirty.remove(&k);
            self.submit(k);
        }
        if self.mesher.in_flight >= limit || self.dirty.is_empty() {
            return;
        }
        let mut list: Vec<(i32, SectionKey)> = self
            .dirty
            .iter()
            .map(|&k| {
                let (dx, dy, dz) = (k.0 - near.0, (k.1 - near.1) * 2, k.2 - near.2);
                (dx * dx + dy * dy + dz * dz, k)
            })
            .collect();
        list.sort_unstable();
        for (_, k) in list.into_iter().take(limit - self.mesher.in_flight) {
            self.dirty.remove(&k);
            self.submit(k);
        }
    }

    fn submit(&mut self, key: SectionKey) {
        let (cx, _, cz) = key;
        if !self.columns.contains_key(&(cx, cz)) {
            return;
        }
        let cols: [Option<Arc<Column>>; 9] = std::array::from_fn(|i| {
            let (dx, dz) = (i as i32 % 3 - 1, i as i32 / 3 - 1);
            self.columns.get(&(cx + dx, cz + dz)).cloned()
        });
        let v = self.versions.entry(key).or_insert(0);
        *v += 1;
        let version = *v;
        self.mesher.submit(Job { key, version, cols });
    }

    pub fn pending(&self) -> usize {
        self.dirty.len() + self.mesher.in_flight
    }
}
