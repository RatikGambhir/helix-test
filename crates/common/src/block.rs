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
    pub const TORCH: u8 = 148;
    pub const DOOR_TOP: u8 = 149;
    pub const DOOR_BOTTOM: u8 = 150;
    pub const LADDER: u8 = 151;
    pub const BED_TOP: u8 = 152;
    pub const BED_SIDE: u8 = 153;
    pub const CHEST_TOP: u8 = 154;
    pub const CHEST_FRONT: u8 = 155;
    pub const CHEST_SIDE: u8 = 156;
    pub const FARMLAND: u8 = 157;
    pub const WHEAT: u8 = 158; // 4 stages: 158..=161
    pub const SAPLING: u8 = 162;
    pub const COPPER_ORE: u8 = 163;
    pub const FURNACE_LIT: u8 = 164;
    pub const RAIL: u8 = 165;
    pub const SEEDS: u8 = 166;
    pub const WHEAT_ITEM: u8 = 167;
    pub const MEAT: u8 = 168; // pork, cooked, beef, steak, chicken, cooked, mutton, cooked: 168..=175
    pub const ROTTEN: u8 = 176;
    pub const BONE: u8 = 177;
    pub const STRING: u8 = 178;
    pub const FEATHER: u8 = 179;
    pub const LEATHER: u8 = 180;
    pub const ARROW: u8 = 181;
    pub const BOW: u8 = 182;
    pub const COPPER_INGOT: u8 = 183;
    pub const AXE: u8 = 184; // 4 tiers
    pub const SHOVEL: u8 = 188; // 4 tiers
    pub const HOE: u8 = 192; // 4 tiers
    pub const ARMOR: u8 = 196; // helmet, chest, legs, boots × iron, diamond: 196..=203
    pub const BUCKET: u8 = 204;
    pub const WATER_BUCKET: u8 = 205;
    pub const LAVA_BUCKET: u8 = 206;
    pub const BONE_MEAL: u8 = 207;
    pub const FLINT: u8 = 208;
    pub const COW: u8 = 209;
    pub const COW_FACE: u8 = 210;
    pub const CHICKEN: u8 = 211;
    pub const CHICKEN_FACE: u8 = 212;
    pub const SKELETON: u8 = 213;
    pub const SKELETON_FACE: u8 = 214;
    pub const SPIDER: u8 = 215;
    pub const SPIDER_FACE: u8 = 216;
    pub const RAIN: u8 = 217;
    pub const SNOWFLAKE: u8 = 218;
    pub const SMOKE: u8 = 219;
    pub const FLAME: u8 = 220;
    pub const COUNT: usize = 221;
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
    /// Not a full cube: drawn from boxes (torches, doors, ladders, beds, chests, rails).
    Shape,
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
    TORCH = 69 => BlockDef { light: 14, ..d("Torch", same(t::TORCH), Shape, 0.0, 69) };
    DOOR = 70 => d("Door", [t::DOOR_BOTTOM, t::DOOR_BOTTOM, t::PLANKS], Shape, 1.5, 70);
    DOOR_71 = 71 => d("Door", [t::DOOR_TOP, t::DOOR_TOP, t::PLANKS], Shape, 1.5, 70);
    DOOR_72 = 72 => d("Door", [t::DOOR_BOTTOM, t::DOOR_BOTTOM, t::PLANKS], Shape, 1.5, 70);
    DOOR_73 = 73 => d("Door", [t::DOOR_TOP, t::DOOR_TOP, t::PLANKS], Shape, 1.5, 70);
    DOOR_74 = 74 => d("Door", [t::DOOR_BOTTOM, t::DOOR_BOTTOM, t::PLANKS], Shape, 1.5, 70);
    DOOR_75 = 75 => d("Door", [t::DOOR_TOP, t::DOOR_TOP, t::PLANKS], Shape, 1.5, 70);
    DOOR_76 = 76 => d("Door", [t::DOOR_BOTTOM, t::DOOR_BOTTOM, t::PLANKS], Shape, 1.5, 70);
    DOOR_77 = 77 => d("Door", [t::DOOR_TOP, t::DOOR_TOP, t::PLANKS], Shape, 1.5, 70);
    LADDER = 78 => d("Ladder", same(t::LADDER), Shape, 0.4, 78);
    LADDER_79 = 79 => d("Ladder", same(t::LADDER), Shape, 0.4, 78);
    LADDER_80 = 80 => d("Ladder", same(t::LADDER), Shape, 0.4, 78);
    LADDER_81 = 81 => d("Ladder", same(t::LADDER), Shape, 0.4, 78);
    BED = 82 => d("Bed", [t::BED_TOP, t::BED_SIDE, t::PLANKS], Shape, 0.3, 82);
    CHEST = 83 => d("Chest", [t::CHEST_TOP, t::CHEST_SIDE, t::CHEST_TOP], Shape, 2.5, 83);
    FARMLAND = 84 => d("Farmland", [t::FARMLAND, t::DIRT, t::DIRT], Shape, 0.6, 2);
    WHEAT_0 = 85 => d("Wheat Crop", same(t::WHEAT), Plant, 0.0, 143);
    WHEAT_1 = 86 => d("Wheat Crop", same(t::WHEAT + 1), Plant, 0.0, 143);
    WHEAT_2 = 87 => d("Wheat Crop", same(t::WHEAT + 2), Plant, 0.0, 143);
    WHEAT_3 = 88 => d("Wheat Crop", same(t::WHEAT + 3), Plant, 0.0, 144);
    SAPLING = 89 => d("Sapling", same(t::SAPLING), Plant, 0.0, 89);
    COPPER_ORE = 90 => BlockDef { needs_pick: true, ..d("Copper Ore", same(t::COPPER_ORE), Solid, 3.0, 90) };
    WATER_1 = 91 => d("Flowing Water", same(t::WATER), Liquid, -1.0, 0);
    WATER_2 = 92 => d("Flowing Water", same(t::WATER), Liquid, -1.0, 0);
    WATER_3 = 93 => d("Flowing Water", same(t::WATER), Liquid, -1.0, 0);
    WATER_4 = 94 => d("Flowing Water", same(t::WATER), Liquid, -1.0, 0);
    WATER_5 = 95 => d("Flowing Water", same(t::WATER), Liquid, -1.0, 0);
    WATER_6 = 96 => d("Flowing Water", same(t::WATER), Liquid, -1.0, 0);
    WATER_7 = 97 => d("Flowing Water", same(t::WATER), Liquid, -1.0, 0);
    LAVA_1 = 98 => BlockDef { light: 13, ..d("Flowing Lava", same(t::LAVA), Liquid, -1.0, 0) };
    LAVA_2 = 99 => BlockDef { light: 14, ..d("Flowing Lava", same(t::LAVA), Liquid, -1.0, 0) };
    LAVA_3 = 100 => BlockDef { light: 15, ..d("Flowing Lava", same(t::LAVA), Liquid, -1.0, 0) };
    FURNACE_LIT = 101 => BlockDef { light: 13, needs_pick: true, ..d("Furnace", [t::FURNACE_SIDE, t::FURNACE_LIT, t::FURNACE_SIDE], Solid, 3.5, 32) };
    RAIL = 102 => d("Rail", same(t::RAIL), Shape, 0.7, 102);
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
pub const SEEDS: BlockId = 143;
pub const WHEAT: BlockId = 144;
pub const RAW_PORK: BlockId = 145;
pub const COOKED_PORK: BlockId = 146;
pub const RAW_BEEF: BlockId = 147;
pub const STEAK: BlockId = 148;
pub const RAW_CHICKEN: BlockId = 149;
pub const COOKED_CHICKEN: BlockId = 150;
pub const RAW_MUTTON: BlockId = 151;
pub const COOKED_MUTTON: BlockId = 152;
pub const ROTTEN_FLESH: BlockId = 153;
pub const BONE: BlockId = 154;
pub const STRING: BlockId = 155;
pub const FEATHER: BlockId = 156;
pub const LEATHER: BlockId = 157;
pub const ARROW: BlockId = 158;
pub const BOW: BlockId = 159;
pub const COPPER_INGOT: BlockId = 160;
pub const WOOD_AXE: BlockId = 161;
pub const DIAMOND_AXE: BlockId = 164;
pub const WOOD_SHOVEL: BlockId = 165;
pub const DIAMOND_SHOVEL: BlockId = 168;
pub const WOOD_HOE: BlockId = 169;
pub const DIAMOND_HOE: BlockId = 172;
pub const IRON_HELMET: BlockId = 173;
pub const IRON_CHESTPLATE: BlockId = 174;
pub const IRON_LEGGINGS: BlockId = 175;
pub const IRON_BOOTS: BlockId = 176;
pub const DIAMOND_HELMET: BlockId = 177;
pub const DIAMOND_CHESTPLATE: BlockId = 178;
pub const DIAMOND_LEGGINGS: BlockId = 179;
pub const DIAMOND_BOOTS: BlockId = 180;
pub const BUCKET: BlockId = 181;
pub const WATER_BUCKET: BlockId = 182;
pub const LAVA_BUCKET: BlockId = 183;
pub const BONE_MEAL: BlockId = 184;
pub const FLINT: BlockId = 185;
pub const ITEM_END: BlockId = 186;

/// An inventory stack: what, how many, and how worn (tools).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Stack {
    pub id: BlockId,
    pub n: u8,
    pub dmg: u16,
}

impl Stack {
    pub fn new(id: BlockId, n: u8) -> Stack {
        Stack { id, n, dmg: 0 }
    }
    pub fn one(self) -> Stack {
        Stack { n: 1, ..self }
    }
    /// Same item and stackable together.
    pub fn same(&self, o: &Stack) -> bool {
        self.id == o.id && self.dmg == o.dmg && max_stack(self.id) > 1
    }
}


/// Tool tier 0..=3 and kind for picks, axes, shovels, hoes and swords.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    Pick,
    Axe,
    Shovel,
    Hoe,
    Sword,
}

pub fn tool(id: BlockId) -> Option<(Tool, u8)> {
    match id {
        WOOD_PICK..=DIAMOND_PICK => Some((Tool::Pick, id - WOOD_PICK)),
        WOOD_SWORD..=DIAMOND_SWORD => Some((Tool::Sword, id - WOOD_SWORD)),
        WOOD_AXE..=DIAMOND_AXE => Some((Tool::Axe, id - WOOD_AXE)),
        WOOD_SHOVEL..=DIAMOND_SHOVEL => Some((Tool::Shovel, id - WOOD_SHOVEL)),
        WOOD_HOE..=DIAMOND_HOE => Some((Tool::Hoe, id - WOOD_HOE)),
        _ => None,
    }
}

/// Uses before a tool breaks (0: doesn't wear).
pub fn max_durability(id: BlockId) -> u16 {
    if id == BOW {
        return 384;
    }
    match tool(id) {
        Some((_, tier)) => [60, 132, 251, 1562][tier as usize],
        None => 0,
    }
}

/// Armor slot (0 helmet, 1 chestplate, 2 leggings, 3 boots) and protection points.
pub fn armor(id: BlockId) -> Option<(usize, u8)> {
    match id {
        IRON_HELMET..=IRON_BOOTS => {
            let s = (id - IRON_HELMET) as usize;
            Some((s, [2, 6, 5, 2][s]))
        }
        DIAMOND_HELMET..=DIAMOND_BOOTS => {
            let s = (id - DIAMOND_HELMET) as usize;
            Some((s, [3, 8, 6, 3][s]))
        }
        _ => None,
    }
}

/// Seconds of furnace burn time an item gives.
pub fn fuel_time(id: BlockId) -> f32 {
    match id {
        COAL => 80.0,
        LAVA_BUCKET => 400.0,
        PLANKS | OAK_LOG | BIRCH_LOG | SPRUCE_LOG | CRAFTING_TABLE | BOOKSHELF | CHEST | LADDER | DOOR => 15.0,
        STICK | SAPLING => 5.0,
        WOOD_PICK | WOOD_SWORD | WOOD_AXE | WOOD_SHOVEL | WOOD_HOE | BOW => 10.0,
        _ => 0.0,
    }
}

/// Door block from its parts. Axis 0: the panel faces ±z when closed; 1: ±x.
pub fn door(upper: bool, open: bool, axis: u8) -> BlockId {
    DOOR + upper as u8 + (open as u8) * 2 + (axis & 1) * 4
}

/// (upper, open, axis) if `id` is a door.
pub fn door_parts(id: BlockId) -> Option<(bool, bool, u8)> {
    (DOOR..=DOOR + 7).contains(&id).then(|| {
        let b = id - DOOR;
        (b & 1 == 1, b & 2 == 2, (b >> 2) & 1)
    })
}

/// Ladder whose plate sits on side `side` of its cell: 0 -z, 1 +x, 2 +z, 3 -x.
pub fn ladder(side: u8) -> BlockId {
    LADDER + (side & 3)
}

pub fn is_ladder(id: BlockId) -> bool {
    (LADDER..=LADDER + 3).contains(&id)
}

/// Fluid kind (0 water, 1 lava) and level (water 1..=8, lava 1..=4; the top level is a source).
pub fn fluid(id: BlockId) -> Option<(u8, u8)> {
    match id {
        WATER => Some((0, 8)),
        WATER_1..=WATER_7 => Some((0, id - WATER_1 + 1)),
        LAVA => Some((1, 4)),
        LAVA_1..=LAVA_3 => Some((1, id - LAVA_1 + 1)),
        _ => None,
    }
}

pub fn fluid_block(kind: u8, level: u8) -> BlockId {
    match (kind, level) {
        (0, 8..) => WATER,
        (0, l) => WATER_1 + l.clamp(1, 7) - 1,
        (_, 4..) => LAVA,
        (_, l) => LAVA_1 + l.clamp(1, 3) - 1,
    }
}

pub fn is_source(id: BlockId) -> bool {
    id == WATER || id == LAVA
}

/// The boxes a shaped block is drawn from, in block units (min xyz, max xyz).
pub fn shape_boxes(id: BlockId) -> &'static [[f32; 6]] {
    const P: f32 = 3.0 / 16.0;
    match id {
        TORCH => &[[7.0 / 16.0, 0.0, 7.0 / 16.0, 9.0 / 16.0, 10.0 / 16.0, 9.0 / 16.0]],
        BED => &[[0.0, 0.0, 0.0, 1.0, 9.0 / 16.0, 1.0]],
        CHEST => &[[1.0 / 16.0, 0.0, 1.0 / 16.0, 15.0 / 16.0, 14.0 / 16.0, 15.0 / 16.0]],
        FARMLAND => &[[0.0, 0.0, 0.0, 1.0, 15.0 / 16.0, 1.0]],
        RAIL => &[[0.0, 0.0, 0.0, 1.0, 1.0 / 16.0, 1.0]],
        _ if is_ladder(id) => match id - LADDER {
            0 => &[[0.0, 0.0, 0.0, 1.0, 1.0, 1.0 / 16.0]],
            1 => &[[15.0 / 16.0, 0.0, 0.0, 1.0, 1.0, 1.0]],
            2 => &[[0.0, 0.0, 15.0 / 16.0, 1.0, 1.0, 1.0]],
            _ => &[[0.0, 0.0, 0.0, 1.0 / 16.0, 1.0, 1.0]],
        },
        _ => match door_parts(id) {
            // Closed: a panel across the middle. Open: swung against the cell's edge.
            Some((_, false, 0)) => &[[0.0, 0.0, 0.5 - P / 2.0, 1.0, 1.0, 0.5 + P / 2.0]],
            Some((_, false, _)) => &[[0.5 - P / 2.0, 0.0, 0.0, 0.5 + P / 2.0, 1.0, 1.0]],
            Some((_, true, 0)) => &[[0.0, 0.0, 0.0, P, 1.0, 1.0]],
            Some((_, true, _)) => &[[0.0, 0.0, 0.0, 1.0, 1.0, P]],
            None => &[[0.0, 0.0, 0.0, 1.0, 1.0, 1.0]],
        },
    }
}

/// What a body collides with in a block's cell.
pub fn collision_boxes(id: BlockId) -> &'static [[f32; 6]] {
    match def(id).kind {
        Kind::Solid | Kind::Cutout => &[[0.0, 0.0, 0.0, 1.0, 1.0, 1.0]],
        Kind::Shape if id == TORCH || id == RAIL || is_ladder(id) => &[],
        Kind::Shape => shape_boxes(id),
        _ => &[],
    }
}

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
    let hidden = |b: BlockId| {
        matches!(b, WATER | LAVA | BEDROCK | FURNACE_LIT | FARMLAND | WHEAT_0..=WHEAT_3 | WATER_1..=WATER_7 | LAVA_1..=LAVA_3)
            || (DOOR + 1..=DOOR + 7).contains(&b)
            || (LADDER + 1..=LADDER + 3).contains(&b)
    };
    (1..BLOCK_COUNT as u8).filter(|&b| !hidden(b)).chain(STICK..ITEM_END).collect()
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
        SEEDS => "Seeds",
        WHEAT => "Wheat",
        RAW_PORK => "Raw Porkchop",
        COOKED_PORK => "Cooked Porkchop",
        RAW_BEEF => "Raw Beef",
        STEAK => "Steak",
        RAW_CHICKEN => "Raw Chicken",
        COOKED_CHICKEN => "Cooked Chicken",
        RAW_MUTTON => "Raw Mutton",
        COOKED_MUTTON => "Cooked Mutton",
        ROTTEN_FLESH => "Rotten Flesh",
        BONE => "Bone",
        STRING => "String",
        FEATHER => "Feather",
        LEATHER => "Leather",
        ARROW => "Arrow",
        BOW => "Bow",
        COPPER_INGOT => "Copper Ingot",
        WOOD_AXE..=DIAMOND_AXE => ["Wooden Axe", "Stone Axe", "Iron Axe", "Diamond Axe"][(id - WOOD_AXE) as usize],
        WOOD_SHOVEL..=DIAMOND_SHOVEL => ["Wooden Shovel", "Stone Shovel", "Iron Shovel", "Diamond Shovel"][(id - WOOD_SHOVEL) as usize],
        WOOD_HOE..=DIAMOND_HOE => ["Wooden Hoe", "Stone Hoe", "Iron Hoe", "Diamond Hoe"][(id - WOOD_HOE) as usize],
        IRON_HELMET => "Iron Helmet",
        IRON_CHESTPLATE => "Iron Chestplate",
        IRON_LEGGINGS => "Iron Leggings",
        IRON_BOOTS => "Iron Boots",
        DIAMOND_HELMET => "Diamond Helmet",
        DIAMOND_CHESTPLATE => "Diamond Chestplate",
        DIAMOND_LEGGINGS => "Diamond Leggings",
        DIAMOND_BOOTS => "Diamond Boots",
        BUCKET => "Bucket",
        WATER_BUCKET => "Water Bucket",
        LAVA_BUCKET => "Lava Bucket",
        BONE_MEAL => "Bone Meal",
        FLINT => "Flint",
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
        SEEDS => tile::SEEDS,
        WHEAT => tile::WHEAT_ITEM,
        RAW_PORK..=COOKED_MUTTON => tile::MEAT + (id - RAW_PORK),
        ROTTEN_FLESH => tile::ROTTEN,
        BONE => tile::BONE,
        STRING => tile::STRING,
        FEATHER => tile::FEATHER,
        LEATHER => tile::LEATHER,
        ARROW => tile::ARROW,
        BOW => tile::BOW,
        COPPER_INGOT => tile::COPPER_INGOT,
        WOOD_AXE..=DIAMOND_AXE => tile::AXE + (id - WOOD_AXE),
        WOOD_SHOVEL..=DIAMOND_SHOVEL => tile::SHOVEL + (id - WOOD_SHOVEL),
        WOOD_HOE..=DIAMOND_HOE => tile::HOE + (id - WOOD_HOE),
        IRON_HELMET..=DIAMOND_BOOTS => {
            let i = id - IRON_HELMET;
            tile::ARMOR + (i % 4) * 2 + i / 4
        }
        BUCKET => tile::BUCKET,
        WATER_BUCKET => tile::WATER_BUCKET,
        LAVA_BUCKET => tile::LAVA_BUCKET,
        BONE_MEAL => tile::BONE_MEAL,
        FLINT => tile::FLINT,
        TORCH => tile::TORCH,
        DOOR => tile::DOOR_TOP,
        LADDER => tile::LADDER,
        SAPLING => tile::SAPLING,
        RAIL => tile::RAIL,
        _ => def(id).tex[1],
    }
}

/// Items drawn as flat sprites in the inventory and in hand.
pub fn is_flat(id: BlockId) -> bool {
    !is_block(id) || matches!(def(id).kind, Kind::Plant) || matches!(id, TORCH | DOOR | LADDER | RAIL)
}

pub fn max_stack(id: BlockId) -> u8 {
    if tool(id).is_some() || armor(id).is_some() || matches!(id, BOW | WATER_BUCKET | LAVA_BUCKET) {
        return 1;
    }
    match id {
        BUCKET | BED | DOOR => 16,
        _ => 64,
    }
}

/// Hunger points restored by eating, if edible.
pub fn food_value(id: BlockId) -> Option<f32> {
    Some(match id {
        APPLE => 4.0,
        BREAD => 5.0,
        RAW_PORK | RAW_BEEF => 3.0,
        RAW_CHICKEN | RAW_MUTTON => 2.0,
        COOKED_PORK | STEAK => 8.0,
        COOKED_CHICKEN | COOKED_MUTTON => 6.0,
        ROTTEN_FLESH => 4.0,
        _ => return None,
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Material {
    Rock,
    Wood,
    Soil,
    Other,
}

fn material(b: BlockId) -> Material {
    match b {
        _ if def(b).needs_pick => Material::Rock,
        OAK_LOG | BIRCH_LOG | SPRUCE_LOG | PLANKS | CRAFTING_TABLE | BOOKSHELF | CHEST | LADDER..=LADDER_81 | DOOR..=DOOR_77 | BED => Material::Wood,
        DIRT | GRASS | SAND | GRAVEL | CLAY | SNOW | FARMLAND => Material::Soil,
        _ => Material::Other,
    }
}

/// Mining speed multiplier for the held item against a block.
pub fn tool_speed(held: BlockId, block: BlockId) -> f32 {
    let Some((kind, tier)) = tool(held) else { return 1.0 };
    let speed = [2.0, 4.0, 6.0, 8.0][tier as usize];
    match (kind, material(block)) {
        (Tool::Pick, Material::Rock) | (Tool::Axe, Material::Wood) | (Tool::Shovel, Material::Soil) => speed,
        (Tool::Sword, _) if matches!(block, OAK_LEAVES | BIRCH_LEAVES | SPRUCE_LEAVES | CHERRY_LEAVES) => 3.0,
        _ => 1.0,
    }
}

pub fn can_harvest(held: BlockId, block: BlockId) -> bool {
    let tier = match tool(held) {
        Some((Tool::Pick, t)) => t + 1,
        _ => 0,
    };
    let need = match block {
        b if !def(b).needs_pick => 0,
        IRON_ORE | IRON_BLOCK | GOLD_ORE | GOLD_BLOCK | COPPER_ORE => 2,
        DIAMOND_ORE | DIAMOND_BLOCK => 3,
        OBSIDIAN => 4,
        _ => 1,
    };
    tier >= need
}

pub fn melee_damage(held: BlockId) -> f32 {
    match tool(held) {
        Some((Tool::Sword, t)) => 4.0 + t as f32,
        Some((Tool::Axe, t)) => 3.0 + t as f32,
        Some((_, t)) => 2.0 + t as f32 * 0.5,
        None => 1.0,
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
        assert_eq!(door_parts(door(true, true, 1)), Some((true, true, 1)));
        assert_eq!(fluid(fluid_block(0, 3)), Some((0, 3)));
        assert_eq!(fluid(fluid_block(1, 4)), Some((1, 4)));
        assert_eq!(item_name(DIAMOND_AXE), "Diamond Axe");
        assert_eq!(armor(DIAMOND_CHESTPLATE), Some((1, 8)));
        assert!((tile::COUNT) <= 256);
    }
}
