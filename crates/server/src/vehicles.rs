//! Vehicles in the world: who rides what, and where the drivers say they are.

use crate::World;
use glam::Vec3;
use runecraft_common::block::Stack;
use runecraft_common::protocol::{ToClient, VehicleKind, VehicleState};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Vehicle {
    pub id: u32,
    pub kind: VehicleKind,
    pub pos: [f32; 3],
    pub yaw: f32,
    #[serde(skip)]
    pub speed: f32,
    #[serde(skip)]
    pub cars: Vec<[f32; 4]>,
    #[serde(skip)]
    pub riders: Vec<(u8, u32)>,
}

impl Vehicle {
    fn state(&self) -> VehicleState {
        VehicleState { id: self.id, kind: self.kind, pos: self.pos, yaw: self.yaw, speed: self.speed, cars: self.cars.clone(), riders: self.riders.clone() }
    }
}

impl World {
    pub(crate) fn vehicles_message(&self) -> ToClient {
        ToClient::Vehicles(self.lv.vehicles.values().map(Vehicle::state).collect())
    }

    fn vehicles_changed(&mut self) {
        let msg = self.vehicles_message();
        self.broadcast_dim(&msg);
    }

    pub(crate) fn place_vehicle(&mut self, kind: VehicleKind, pos: [f32; 3], yaw: f32) {
        if self.lv.vehicles.len() > 400 {
            return;
        }
        let id = self.next_item;
        self.next_item += 1;
        self.lv.vehicles.insert(id, Vehicle { id, kind, pos, yaw, speed: 0.0, cars: Vec::new(), riders: Vec::new() });
        self.dirty = true;
        self.vehicles_changed();
    }

    pub(crate) fn mount(&mut self, conn: u32, id: u32, seat: u8) {
        let Some(me) = self.players.get(&conn) else { return };
        let (at, old) = (me.pos(), me.riding);
        let Some(v) = self.lv.vehicles.get(&id) else { return };
        if Vec3::from(v.pos).distance(at) > 8.0 {
            return;
        }
        let taken = |s: u8| v.riders.iter().any(|r| r.0 == s);
        let Some(seat) = std::iter::once(seat).chain(0..v.kind.seats()).find(|&s| s < v.kind.seats() && !taken(s)) else {
            self.tell(conn, "That's full.");
            return;
        };
        self.dismount(conn, old);
        if let Some(v) = self.lv.vehicles.get_mut(&id) {
            v.riders.push((seat, conn));
        }
        if let Some(p) = self.players.get_mut(&conn) {
            p.riding = Some((id, seat));
        }
        self.vehicles_changed();
    }

    pub(crate) fn dismount(&mut self, conn: u32, riding: Option<(u32, u8)>) {
        let Some((id, _)) = riding else { return };
        if let Some(v) = self.lv.vehicles.get_mut(&id) {
            v.riders.retain(|r| r.1 != conn);
            if v.riders.iter().all(|r| r.0 != 0) {
                v.speed = 0.0;
            }
        }
        if let Some(p) = self.players.get_mut(&conn) {
            p.riding = None;
        }
        self.dirty = true;
        self.vehicles_changed();
    }

    pub(crate) fn vehicle_moved(&mut self, conn: u32, id: u32, pos: [f32; 3], yaw: f32, speed: f32, cars: Vec<[f32; 4]>) {
        let Some(v) = self.lv.vehicles.get_mut(&id) else { return };
        if !v.riders.contains(&(0, conn)) || !pos.iter().all(|c| c.is_finite()) {
            return;
        }
        v.pos = pos;
        v.yaw = yaw;
        v.speed = speed;
        v.cars = cars.into_iter().take(v.kind.cars()).collect();
        self.lv.vehicles_dirty = true;
    }

    pub(crate) fn break_vehicle(&mut self, conn: u32, id: u32) {
        let Some(at) = self.players.get(&conn).map(|p| p.pos()) else { return };
        let Some(v) = self.lv.vehicles.get(&id) else { return };
        if !v.riders.is_empty() || Vec3::from(v.pos).distance(at) > 8.0 {
            return;
        }
        let v = self.lv.vehicles.remove(&id).unwrap();
        self.spawn_item(Stack::new(v.kind.item(), 1), Vec3::from(v.pos) + Vec3::Y * 0.5, Vec3::Y * 2.0, 0.3);
        self.dirty = true;
        self.vehicles_changed();
    }

    pub(crate) fn tick_vehicles(&mut self, dt: f32) {
        self.lv.vehicle_clock += dt;
        let due = (self.lv.vehicles_dirty && self.lv.vehicle_clock > 0.05) || self.lv.vehicle_clock > 2.0;
        if due && (!self.lv.vehicles.is_empty() || self.lv.vehicles_dirty) {
            self.lv.vehicle_clock = 0.0;
            self.lv.vehicles_dirty = false;
            self.vehicles_changed();
        }
    }
}
