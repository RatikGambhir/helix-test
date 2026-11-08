//! Messages between client and server. Each travels as a length-prefixed bincode frame.

use crate::block::Stack;
pub use crate::vehicle::{VehicleKind, VehicleState};
use serde::{Deserialize, Serialize};

pub type PlayerId = u32;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct Pose {
    pub pos: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
}

/// An item stack lying in the world.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct ItemState {
    pub id: u32,
    pub item: u8,
    pub pos: [f32; 3],
}

/// Everything about a player that survives leaving the game.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct PlayerSave {
    pub inv: Vec<Option<Stack>>,
    pub armor: Vec<Option<Stack>>,
    pub health: f32,
    pub hunger: f32,
    pub creative: bool,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ContainerKind {
    Chest,
    Furnace,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ToServer {
    Hello { name: String, version: u32, view_distance: u8, skin: u8 },
    Move(Pose),
    SetBlock { pos: [i32; 3], block: u8 },
    Chat(String),
    /// Light a TNT block.
    Ignite { pos: [i32; 3] },
    ViewDistance(u8),
    /// Ask to be moved (city travel, respawn); the server answers with Teleport.
    Travel { pos: [f32; 3] },
    /// Throw items into the world.
    Drop { stack: Stack, pos: [f32; 3], vel: [f32; 3] },
    OpenContainer { pos: [i32; 3] },
    ContainerSet { pos: [i32; 3], slot: u8, stack: Option<Stack> },
    CloseContainer,
    /// Lie down in (Some) or get out of (None) a bed.
    Sleep(Option<[i32; 3]>),
    Held(u8),
    Save(PlayerSave),
    /// Strike flint and steel in an empty cell: lights a portal frame around it.
    Light { pos: [i32; 3] },
    PlaceVehicle { kind: VehicleKind, pos: [f32; 3], yaw: f32 },
    /// Get into a vehicle's seat (0 drives).
    Mount { id: u32, seat: u8 },
    Dismount,
    /// The driver reports where the vehicle (and the cars it pulls) went.
    VehicleMove { id: u32, pos: [f32; 3], yaw: f32, speed: f32, cars: Vec<[f32; 4]> },
    /// Break a vehicle back into its item.
    BreakVehicle { id: u32 },
    /// Download a real place from OpenStreetMap and build it in the world.
    LoadPlace(String),
}

/// A real place built from map data, for the travel list.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlaceSummary {
    pub name: String,
    pub pos: [f32; 3],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ToClient {
    Welcome { id: PlayerId, seed: u64, dim: u8, spawn: [f32; 3], time: f32, world: String, motd: String, save: Option<PlayerSave> },
    Column { cx: i32, cz: i32, data: Vec<u8> },
    Unload { cx: i32, cz: i32 },
    BlockChanged { pos: [i32; 3], block: u8 },
    PlayerJoined { id: PlayerId, name: String, skin: u8, pose: Pose },
    PlayerLeft { id: PlayerId },
    PlayerMoved { id: PlayerId, pose: Pose },
    PlayerHeld { id: PlayerId, item: u8 },
    Chat { from: Option<String>, text: String },
    Time(f32),
    Teleport { pos: [f32; 3] },
    Items(Vec<ItemState>),
    /// Something hurt you (an explosion or lightning).
    Damage { amount: f32, from: [f32; 3] },
    Explosion { pos: [f32; 3] },
    /// Items you walked over.
    Pickup(Stack),
    Container { pos: [i32; 3], kind: ContainerKind, slots: Vec<Option<Stack>>, burn: f32, progress: f32 },
    Weather { rain: f32, thunder: bool },
    Lightning { pos: [f32; 3] },
    /// Where you respawn (set by sleeping in a bed).
    SpawnPoint([f32; 3]),
    /// Morning came: everyone in bed gets up.
    WakeUp,
    Kicked(String),
    /// You went through a portal: forget the old world, these chunks belong to dimension `dim`.
    Dimension { dim: u8, pos: [f32; 3] },
    Vehicles(Vec<VehicleState>),
    Places(Vec<PlaceSummary>),
}
