//! Every block and item in the game, with how it looks, feels and drops.

pub type BlockId = u8;

/// Texture tiles in the 16×16-tile atlas. The client paints each one procedurally.
pub mod tile {
    pub const STONE: u8 = 0;
    pub const DIRT: u8 = 1;
    pub const GRASS_TOP: u8 = 2;
    pub const GRASS_SIDE: u8 = 3;
    pub const SAND: u8 = 4;
    pub const GRAVEL: u8 = 5;
    pub const WATER: u8 = 6;
    pub const BEDROCK: u8 = 7;
    pub const OAK_LOG: u8 = 8;
    pub const OAK_LOG_TOP: u8 = 9;
    pub const OAK_LEAVES: u8 = 10;
    pub const PLANKS: u8 = 11;
    pub const COBBLE: u8 = 12;
    pub const GLASS: u8 = 13;
    pub const BRICK: u8 = 14;
    pub const COAL_ORE: u8 = 15;
    pub const IRON_ORE: u8 = 16;
    pub const GOLD_ORE: u8 = 17;
    pub const DIAMOND_ORE: u8 = 18;
    pub const SNOW: u8 = 19;
    pub const ICE: u8 = 20;
    pub const CACTUS_SIDE: u8 = 21;
    pub const CACTUS_TOP: u8 = 22;
    pub const CLAY: u8 = 23;
    pub const SANDSTONE: u8 = 24;
    pub const SANDSTONE_TOP: u8 = 25;
    pub const BIRCH_LOG: u8 = 26;
    pub const BIRCH_LEAVES: u8 = 27;
    pub const SPRUCE_LOG: u8 = 28;
    pub const SPRUCE_LEAVES: u8 = 29;
    pub const CHERRY_LEAVES: u8 = 30;
    pub const TALL_GRASS: u8 = 31;
    pub const FLOWER_RED: u8 = 32;
    pub const FLOWER_YELLOW: u8 = 33;
    pub const TABLE_TOP: u8 = 34;
    pub const TABLE_SIDE: u8 = 35;
    pub const FURNACE_FRONT: u8 = 36;
    pub const FURNACE_SIDE: u8 = 37;
    pub const BOOKSHELF: u8 = 38;
    pub const GLOWSTONE: u8 = 39;
    pub const LANTERN: u8 = 40;
    pub const OBSIDIAN: u8 = 41;
    pub const TNT_SIDE: u8 = 42;
    pub const TNT_TOP: u8 = 43;
    pub const WOOL: u8 = 44; // 6 colours: 44..=49
    pub const CONCRETE: u8 = 50; // 3 shades: 50..=52
    pub const ASPHALT: u8 = 53;
    pub const ROAD_LINE: u8 = 54;
    pub const ROAD_WHITE: u8 = 55;
    pub const SIDEWALK: u8 = 56;
    pub const WINDOW_BLUE: u8 = 57;
    pub const STEEL: u8 = 58;
    pub const RED_STEEL: u8 = 59;
    pub const COPPER: u8 = 60;
    pub const GOLD_BLOCK: u8 = 61;
    pub const LIMESTONE: u8 = 62;
    pub const SLATE: u8 = 63;
    pub const NEON: u8 = 64; // 3 colours: 64..=66
    pub const IRON_BLOCK: u8 = 67;
    pub const DIAMOND_BLOCK: u8 = 68;
    pub const STONE_BRICK: u8 = 69;
    pub const TERRACOTTA: u8 = 70;
    pub const WINDOW_DARK: u8 = 71;
    pub const MARBLE: u8 = 72;
    pub const BIRCH_LOG_TOP: u8 = 73;
    pub const SPRUCE_LOG_TOP: u8 = 74;
    pub const WINDOW_WARM: u8 = 75;
    pub const LAVA: u8 = 76;
    // Item sprites
    pub const STICK: u8 = 96;
    pub const COAL: u8 = 97;
    pub const IRON_INGOT: u8 = 98;
    pub const GOLD_INGOT: u8 = 99;
    pub const DIAMOND: u8 = 100;
    pub const PICK: u8 = 101; // 4 tiers: 101..=104
    pub const APPLE: u8 = 105;
    pub const BREAD: u8 = 106;
    pub const SWORD: u8 = 107; // 4 tiers: 107..=110
    // Utility tiles
    pub const CRACK: u8 = 112; // 10 stages: 112..=121
    pub const WHITE: u8 = 122;
    pub const BLACK: u8 = 123;
    pub const SKIN: u8 = 124; // 4 skins: 124..=127
    pub const SHIRT: u8 = 128; // 8 shirts: 128..=135
    pub const PANTS: u8 = 136;
    pub const FACE: u8 = 137; // 4 faces: 137..=140
    pub const HAIR: u8 = 141;
    pub const PIG: u8 = 142;
    pub const PIG_FACE: u8 = 143;
    pub const SHEEP: u8 = 144;
    pub const SHEEP_FACE: u8 = 145;
    pub const ZOMBIE: u8 = 146;
    pub const ZOMBIE_FACE: u8 = 147;
    pub const COUNT: usize = 148;
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Air,
    /// Opaque cube.
    Solid,
    /// Cube with see-through pixels (glass, leaves). Culls only against its own kind.
    Cutout,
    /// Translucent liquid, swimmable.
    Liquid,
    /// Crossed sprites, no collision.
    Plant,
}

#[derive(Clone, Copy, Debug)]
pub struct BlockDef {
    pub name: &'static str,
    /// Tiles for top, sides, bottom.
    pub tex: [u8; 3],
    pub kind: Kind,
    /// Light emitted, 0–15.
    pub light: u8,
    /// Seconds to break by hand. Negative: unbreakable.
    pub hardness: f32,
    /// What breaking it gives in survival (AIR: nothing).
    pub drop: BlockId,
    /// Needs a pickaxe to drop anything.
    pub needs_pick: bool,
}

const fn d(name: &'static str, tex: [u8; 3], kind: Kind, hardness: f32, drop: BlockId) -> BlockDef {
    BlockDef { name, tex, kind, light: 0, hardness, drop, needs_pick: false }
}
const fn same(t: u8) -> [u8; 3] {
    [t, t, t]
}

macro_rules! blocks {
    ($($id:ident = $n:expr => $def:expr;)*) => {
        $(pub const $id: BlockId = $n;)*
        pub const BLOCK_COUNT: usize = 0 $(+ { let _ = $n; 1 })*;
        pub static BLOCKS: [BlockDef; BLOCK_COUNT] = [$($def),*];
    };
}

use tile as t;
use Kind::*;

blocks! {
    AIR = 0 => d("Air", same(0), Air, 0.0, 0);
    STONE = 1 => BlockDef { needs_pick: true, ..d("Stone", same(t::STONE), Solid, 1.5, 11) };
    DIRT = 2 => d("Dirt", same(t::DIRT), Solid, 0.5, 2);
    GRASS = 3 => d("Grass Block", [t::GRASS_TOP, t::GRASS_SIDE, t::DIRT], Solid, 0.6, 2);
    SAND = 4 => d("Sand", same(t::SAND), Solid, 0.5, 4);
    GRAVEL = 5 => d("Gravel", same(t::GRAVEL), Solid, 0.6, 5);
    WATER = 6 => d("Water", same(t::WATER), Liquid, -1.0, 0);
    BEDROCK = 7 => d("Bedrock", same(t::BEDROCK), Solid, -1.0, 0);
    OAK_LOG = 8 => d("Oak Log", [t::OAK_LOG_TOP, t::OAK_LOG, t::OAK_LOG_TOP], Solid, 2.0, 8);
    OAK_LEAVES = 9 => d("Oak Leaves", same(t::OAK_LEAVES), Cutout, 0.2, 0);
    PLANKS = 10 => d("Planks", same(t::PLANKS), Solid, 2.0, 10);
    COBBLE = 11 => BlockDef { needs_pick: true, ..d("Cobblestone", same(t::COBBLE), Solid, 2.0, 11) };
    GLASS = 12 => d("Glass", same(t::GLASS), Cutout, 0.3, 0);
    BRICK = 13 => BlockDef { needs_pick: true, ..d("Bricks", same(t::BRICK), Solid, 2.0, 13) };
    COAL_ORE = 14 => BlockDef { needs_pick: true, ..d("Coal Ore", same(t::COAL_ORE), Solid, 3.0, 129) };
    IRON_ORE = 15 => BlockDef { needs_pick: true, ..d("Iron Ore", same(t::IRON_ORE), Solid, 3.0, 15) };
    GOLD_ORE = 16 => BlockDef { needs_pick: true, ..d("Gold Ore", same(t::GOLD_ORE), Solid, 3.0, 16) };
    DIAMOND_ORE = 17 => BlockDef { needs_pick: true, ..d("Diamond Ore", same(t::DIAMOND_ORE), Solid, 3.0, 132) };
    SNOW = 18 => d("Snow", same(t::SNOW), Solid, 0.3, 18);
    ICE = 19 => d("Ice", same(t::ICE), Solid, 0.5, 0);
    CACTUS = 20 => d("Cactus", [t::CACTUS_TOP, t::CACTUS_SIDE, t::CACTUS_TOP], Solid, 0.4, 20);
    CLAY = 21 => d("Clay", same(t::CLAY), Solid, 0.6, 21);
    SANDSTONE = 22 => BlockDef { needs_pick: true, ..d("Sandstone", [t::SANDSTONE_TOP, t::SANDSTONE, t::SANDSTONE_TOP], Solid, 0.8, 22) };
    BIRCH_LOG = 23 => d("Birch Log", [t::BIRCH_LOG_TOP, t::BIRCH_LOG, t::BIRCH_LOG_TOP], Solid, 2.0, 23);
    BIRCH_LEAVES = 24 => d("Birch Leaves", same(t::BIRCH_LEAVES), Cutout, 0.2, 0);
    SPRUCE_LOG = 25 => d("Spruce Log", [t::SPRUCE_LOG_TOP, t::SPRUCE_LOG, t::SPRUCE_LOG_TOP], Solid, 2.0, 25);
    SPRUCE_LEAVES = 26 => d("Spruce Leaves", same(t::SPRUCE_LEAVES), Cutout, 0.2, 0);
    CHERRY_LEAVES = 27 => d("Cherry Blossom", same(t::CHERRY_LEAVES), Cutout, 0.2, 0);
    TALL_GRASS = 28 => d("Tall Grass", same(t::TALL_GRASS), Plant, 0.0, 0);
    FLOWER_RED = 29 => d("Poppy", same(t::FLOWER_RED), Plant, 0.0, 29);
    FLOWER_YELLOW = 30 => d("Dandelion", same(t::FLOWER_YELLOW), Plant, 0.0, 30);
    CRAFTING_TABLE = 31 => d("Crafting Table", [t::TABLE_TOP, t::TABLE_SIDE, t::PLANKS], Solid, 2.5, 31);
    FURNACE = 32 => BlockDef { needs_pick: true, ..d("Furnace", [t::FURNACE_SIDE, t::FURNACE_FRONT, t::FURNACE_SIDE], Solid, 3.5, 32) };
    BOOKSHELF = 33 => d("Bookshelf", [t::PLANKS, t::BOOKSHELF, t::PLANKS], Solid, 1.5, 33);
    GLOWSTONE = 34 => BlockDef { light: 15, ..d("Glowstone", same(t::GLOWSTONE), Solid, 0.3, 34) };
    LANTERN = 35 => BlockDef { light: 14, ..d("Lantern", same(t::LANTERN), Solid, 0.3, 35) };
    OBSIDIAN = 36 => BlockDef { needs_pick: true, ..d("Obsidian", same(t::OBSIDIAN), Solid, 15.0, 36) };
    TNT = 37 => d("TNT", [t::TNT_TOP, t::TNT_SIDE, t::TNT_TOP], Solid, 0.1, 37);
    WOOL_WHITE = 38 => d("White Wool", same(t::WOOL), Solid, 0.8, 38);
    WOOL_RED = 39 => d("Red Wool", same(t::WOOL + 1), Solid, 0.8, 39);
    WOOL_BLUE = 40 => d("Blue Wool", same(t::WOOL + 2), Solid, 0.8, 40);
    WOOL_GREEN = 41 => d("Green Wool", same(t::WOOL + 3), Solid, 0.8, 41);
    WOOL_YELLOW = 42 => d("Yellow Wool", same(t::WOOL + 4), Solid, 0.8, 42);
    WOOL_BLACK = 43 => d("Black Wool", same(t::WOOL + 5), Solid, 0.8, 43);
    CONCRETE_WHITE = 44 => BlockDef { needs_pick: true, ..d("White Concrete", same(t::CONCRETE), Solid, 1.8, 44) };
    CONCRETE_GRAY = 45 => BlockDef { needs_pick: true, ..d("Gray Concrete", same(t::CONCRETE + 1), Solid, 1.8, 45) };
    CONCRETE_DARK = 46 => BlockDef { needs_pick: true, ..d("Dark Concrete", same(t::CONCRETE + 2), Solid, 1.8, 46) };
    ASPHALT = 47 => BlockDef { needs_pick: true, ..d("Asphalt", same(t::ASPHALT), Solid, 1.5, 47) };
    ROAD_LINE = 48 => BlockDef { needs_pick: true, ..d("Road Line", [t::ROAD_LINE, t::ASPHALT, t::ASPHALT], Solid, 1.5, 47) };
    ROAD_WHITE = 49 => BlockDef { needs_pick: true, ..d("Crosswalk", [t::ROAD_WHITE, t::ASPHALT, t::ASPHALT], Solid, 1.5, 47) };
    SIDEWALK = 50 => BlockDef { needs_pick: true, ..d("Sidewalk", same(t::SIDEWALK), Solid, 1.5, 50) };
    WINDOW_BLUE = 51 => d("Blue Glass Wall", same(t::WINDOW_BLUE), Solid, 0.4, 0);
    STEEL = 52 => BlockDef { needs_pick: true, ..d("Steel", same(t::STEEL), Solid, 4.0, 52) };
    RED_STEEL = 53 => BlockDef { needs_pick: true, ..d("Red Steel", same(t::RED_STEEL), Solid, 4.0, 53) };
    COPPER = 54 => BlockDef { needs_pick: true, ..d("Oxidized Copper", same(t::COPPER), Solid, 3.0, 54) };
    GOLD_BLOCK = 55 => BlockDef { needs_pick: true, ..d("Gold Block", same(t::GOLD_BLOCK), Solid, 3.0, 55) };
    LIMESTONE = 56 => BlockDef { needs_pick: true, ..d("Limestone", same(t::LIMESTONE), Solid, 1.5, 56) };
    SLATE = 57 => BlockDef { needs_pick: true, ..d("Slate Roof", same(t::SLATE), Solid, 1.5, 57) };
    NEON_PINK = 58 => BlockDef { light: 12, ..d("Pink Neon", same(t::NEON), Solid, 0.3, 58) };
    NEON_CYAN = 59 => BlockDef { light: 12, ..d("Cyan Neon", same(t::NEON + 1), Solid, 0.3, 59) };
    NEON_YELLOW = 60 => BlockDef { light: 12, ..d("Yellow Neon", same(t::NEON + 2), Solid, 0.3, 60) };
    IRON_BLOCK = 61 => BlockDef { needs_pick: true, ..d("Iron Block", same(t::IRON_BLOCK), Solid, 4.0, 61) };
    DIAMOND_BLOCK = 62 => BlockDef { needs_pick: true, ..d("Diamond Block", same(t::DIAMOND_BLOCK), Solid, 4.0, 62) };
    STONE_BRICK = 63 => BlockDef { needs_pick: true, ..d("Stone Bricks", same(t::STONE_BRICK), Solid, 1.5, 63) };
    TERRACOTTA = 64 => BlockDef { needs_pick: true, ..d("Terracotta", same(t::TERRACOTTA), Solid, 1.2, 64) };
    WINDOW_DARK = 65 => d("Dark Glass Wall", same(t::WINDOW_DARK), Solid, 0.4, 0);
    MARBLE = 66 => BlockDef { needs_pick: true, ..d("Marble", same(t::MARBLE), Solid, 1.5, 66) };
    WINDOW_WARM = 67 => BlockDef { light: 9, ..d("Lit Window", same(t::WINDOW_WARM), Solid, 0.4, 0) };
    LAVA = 68 => BlockDef { light: 15, ..d("Lava", same(t::LAVA), Liquid, -1.0, 0) };
}

// Items that aren't blocks live at 128 and up.
pub const STICK: BlockId = 128;
pub const COAL: BlockId = 129;
pub const IRON_INGOT: BlockId = 130;
pub const GOLD_INGOT: BlockId = 131;
pub const DIAMOND: BlockId = 132;
pub const WOOD_PICK: BlockId = 133;
pub const STONE_PICK: BlockId = 134;
pub const IRON_PICK: BlockId = 135;
pub const DIAMOND_PICK: BlockId = 136;
pub const APPLE: BlockId = 137;
pub const BREAD: BlockId = 138;
pub const WOOD_SWORD: BlockId = 139;
pub const STONE_SWORD: BlockId = 140;
pub const IRON_SWORD: BlockId = 141;
pub const DIAMOND_SWORD: BlockId = 142;
pub const ITEM_END: BlockId = 143;

#[inline]
pub fn def(id: BlockId) -> &'static BlockDef {
    BLOCKS.get(id as usize).unwrap_or(&BLOCKS[0])
}

pub fn is_block(id: BlockId) -> bool {
    (id as usize) < BLOCK_COUNT && id != AIR
}

pub fn is_item(id: BlockId) -> bool {
    (STICK..ITEM_END).contains(&id)
}

/// Blocks you can place from the creative palette, in display order.
pub fn creative_palette() -> Vec<BlockId> {
    (1..BLOCK_COUNT as u8).filter(|&b| b != WATER && b != LAVA && b != BEDROCK).chain([WATER, LAVA, BEDROCK]).collect()
}

pub fn item_name(id: BlockId) -> &'static str {
    match id {
        STICK => "Stick",
        COAL => "Coal",
        IRON_INGOT => "Iron Ingot",
        GOLD_INGOT => "Gold Ingot",
        DIAMOND => "Diamond",
        WOOD_PICK => "Wooden Pickaxe",
        STONE_PICK => "Stone Pickaxe",
        IRON_PICK => "Iron Pickaxe",
        DIAMOND_PICK => "Diamond Pickaxe",
        APPLE => "Apple",
        BREAD => "Bread",
        WOOD_SWORD => "Wooden Sword",
        STONE_SWORD => "Stone Sword",
        IRON_SWORD => "Iron Sword",
        DIAMOND_SWORD => "Diamond Sword",
        _ => def(id).name,
    }
}

/// The sprite tile for a non-block item.
pub fn item_tile(id: BlockId) -> u8 {
    match id {
        STICK => tile::STICK,
        COAL => tile::COAL,
        IRON_INGOT => tile::IRON_INGOT,
        GOLD_INGOT => tile::GOLD_INGOT,
        DIAMOND => tile::DIAMOND,
        WOOD_PICK..=DIAMOND_PICK => tile::PICK + (id - WOOD_PICK),
        APPLE => tile::APPLE,
        BREAD => tile::BREAD,
        WOOD_SWORD..=DIAMOND_SWORD => tile::SWORD + (id - WOOD_SWORD),
        _ => def(id).tex[1],
    }
}

pub fn max_stack(id: BlockId) -> u8 {
    match id {
        WOOD_PICK..=DIAMOND_PICK | WOOD_SWORD..=DIAMOND_SWORD => 1,
        _ => 64,
    }
}

/// Hunger points restored by eating, if edible.
pub fn food_value(id: BlockId) -> Option<f32> {
    match id {
        APPLE => Some(4.0),
        BREAD => Some(5.0),
        _ => None,
    }
}

/// Mining speed multiplier for the held item against a block.
pub fn tool_speed(held: BlockId, block: BlockId) -> f32 {
    let pick = match held {
        WOOD_PICK => 2.0,
        STONE_PICK => 4.0,
        IRON_PICK => 6.0,
        DIAMOND_PICK => 8.0,
        _ => 1.0,
    };
    if def(block).needs_pick { pick } else { 1.0 }
}

pub fn can_harvest(held: BlockId, block: BlockId) -> bool {
    let tier = match held {
        WOOD_PICK => 1,
        STONE_PICK => 2,
        IRON_PICK => 3,
        DIAMOND_PICK => 4,
        _ => 0,
    };
    let need = match block {
        b if !def(b).needs_pick => 0,
        IRON_ORE | IRON_BLOCK | GOLD_ORE | GOLD_BLOCK => 2,
        DIAMOND_ORE | DIAMOND_BLOCK => 3,
        OBSIDIAN => 4,
        _ => 1,
    };
    tier >= need
}

pub fn melee_damage(held: BlockId) -> f32 {
    match held {
        WOOD_SWORD => 4.0,
        STONE_SWORD => 5.0,
        IRON_SWORD => 6.0,
        DIAMOND_SWORD => 7.0,
        WOOD_PICK..=DIAMOND_PICK => 2.5,
        _ => 1.0,
    }
}

#[inline]
pub fn is_solid(id: BlockId) -> bool {
    matches!(def(id).kind, Kind::Solid | Kind::Cutout)
}

#[inline]
pub fn is_opaque(id: BlockId) -> bool {
    def(id).kind == Kind::Solid
}

#[inline]
pub fn is_liquid(id: BlockId) -> bool {
    def(id).kind == Kind::Liquid
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_match_table_positions() {
        assert_eq!(BLOCKS.len(), BLOCK_COUNT);
        assert_eq!(def(LAVA).name, "Lava");
        assert_eq!(def(STONE).drop, COBBLE);
        assert_eq!(def(COAL_ORE).drop, COAL);
        assert!(BLOCK_COUNT < STICK as usize);
    }
}
