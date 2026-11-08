//! Messages between client and server. Each travels as a length-prefixed bincode frame.

use crate::block::Stack;
use serde::{Deserialize, Serialize};

pub type PlayerId = u32;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct Pose {
    pub pos: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum EntityKind {
    Pig,
    Sheep,
    Cow,
    Chicken,
    Zombie,
    Skeleton,
    Spider,
    /// City folk who walk the streets and chat.
    Citizen,
    Arrow,
    /// A dropped item stack.
    Item,
}

impl EntityKind {
    pub fn hostile(self) -> bool {
        matches!(self, EntityKind::Zombie | EntityKind::Skeleton | EntityKind::Spider)
    }
    pub fn living(self) -> bool {
        !matches!(self, EntityKind::Arrow | EntityKind::Item)
    }
    pub fn name(self) -> &'static str {
        match self {
            EntityKind::Pig => "Pig",
            EntityKind::Sheep => "Sheep",
            EntityKind::Cow => "Cow",
            EntityKind::Chicken => "Chicken",
            EntityKind::Zombie => "Zombie",
            EntityKind::Skeleton => "Skeleton",
            EntityKind::Spider => "Spider",
            EntityKind::Citizen => "Citizen",
            EntityKind::Arrow => "Arrow",
            EntityKind::Item => "Item",
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct EntityState {
    pub id: u32,
    pub kind: EntityKind,
    pub pos: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub hurt: bool,
    /// Item id for dropped items; look variant for citizens.
    pub item: u8,
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
    /// Hit a creature with the held item.
    Attack { entity: u32, damage: f32 },
    /// Right-click a creature (talk to citizens).
    Interact { entity: u32 },
    ViewDistance(u8),
    /// Ask to be moved (city travel, respawn); the server answers with Teleport.
    Travel { pos: [f32; 3] },
    /// Throw items into the world.
    Drop { stack: Stack, pos: [f32; 3], vel: [f32; 3] },
    /// Fire an arrow.
    Shoot { pos: [f32; 3], vel: [f32; 3] },
    OpenContainer { pos: [i32; 3] },
    ContainerSet { pos: [i32; 3], slot: u8, stack: Option<Stack> },
    CloseContainer,
    /// Lie down in (Some) or get out of (None) a bed.
    Sleep(Option<[i32; 3]>),
    /// Grow a crop or sapling right away.
    BoneMeal { pos: [i32; 3] },
    Held(u8),
    Save(PlayerSave),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ToClient {
    Welcome { id: PlayerId, seed: u64, spawn: [f32; 3], time: f32, world: String, motd: String, save: Option<PlayerSave> },
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
    Entities(Vec<EntityState>),
    /// Something hurt you (mob, arrow, explosion, lightning).
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
}
