//! Messages between client and server. Each travels as a length-prefixed bincode frame.

use serde::{Deserialize, Serialize};

pub type PlayerId = u32;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct Pose {
    pub pos: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum MobKind {
    Pig,
    Sheep,
    Zombie,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ToServer {
    Hello { name: String, version: u32, view_distance: u8, skin: u8 },
    Move(Pose),
    SetBlock { pos: [i32; 3], block: u8 },
    Chat(String),
    /// Light a TNT block.
    Ignite { pos: [i32; 3] },
    /// Hit a mob with the held item.
    Attack { mob: u32, damage: f32 },
    ViewDistance(u8),
    /// Ask to be moved (city travel, /tp, respawn); the server answers with Teleport.
    Travel { pos: [f32; 3] },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ToClient {
    Welcome { id: PlayerId, seed: u64, spawn: [f32; 3], time: f32, world: String, motd: String },
    Column { cx: i32, cz: i32, data: Vec<u8> },
    Unload { cx: i32, cz: i32 },
    BlockChanged { pos: [i32; 3], block: u8 },
    PlayerJoined { id: PlayerId, name: String, skin: u8, pose: Pose },
    PlayerLeft { id: PlayerId },
    PlayerMoved { id: PlayerId, pose: Pose },
    Chat { from: Option<String>, text: String },
    Time(f32),
    Teleport { pos: [f32; 3] },
    Mobs(Vec<MobState>),
    /// Something hurt you (zombie, explosion).
    Damage { amount: f32, from: [f32; 3] },
    Explosion { pos: [f32; 3] },
    Kicked(String),
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct MobState {
    pub id: u32,
    pub kind: MobKind,
    pub pos: [f32; 3],
    pub yaw: f32,
    pub hurt: bool,
}
