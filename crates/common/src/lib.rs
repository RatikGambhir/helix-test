//! Shared game logic for RuneCraft: blocks, world generation, cities, physics, crafting and the
//! network protocol. Both the server and the client build on this crate.

pub mod block;
pub mod chunk;
pub mod city;
pub mod craft;
pub mod worldgen;
pub mod net;
pub mod noise;
pub mod physics;
pub mod protocol;

pub const GAME_NAME: &str = "RuneCraft";
pub const DEFAULT_PORT: u16 = 25575;
pub const PROTOCOL_VERSION: u32 = 3;
/// Real seconds for a full day.
pub const DAY_LENGTH: f32 = 1200.0;
