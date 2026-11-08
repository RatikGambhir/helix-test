//! Crafting and smelting recipes.

use crate::block::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Station {
    Hand,
    /// Needs a crafting table within reach.
    Table,
    /// Needs a furnace within reach.
    Furnace,
}

#[derive(Debug)]
pub struct Recipe {
    pub out: BlockId,
    pub count: u8,
    pub inputs: &'static [(BlockId, u8)],
    pub station: Station,
}

const fn r(out: BlockId, count: u8, inputs: &'static [(BlockId, u8)], station: Station) -> Recipe {
    Recipe { out, count, inputs, station }
}

use Station::*;

pub static RECIPES: &[Recipe] = &[
    r(PLANKS, 4, &[(OAK_LOG, 1)], Hand),
    r(PLANKS, 4, &[(BIRCH_LOG, 1)], Hand),
    r(PLANKS, 4, &[(SPRUCE_LOG, 1)], Hand),
    r(STICK, 4, &[(PLANKS, 2)], Hand),
    r(CRAFTING_TABLE, 1, &[(PLANKS, 4)], Hand),
    r(LANTERN, 4, &[(COAL, 1), (STICK, 1)], Hand),
    r(WOOD_PICK, 1, &[(PLANKS, 3), (STICK, 2)], Table),
    r(STONE_PICK, 1, &[(COBBLE, 3), (STICK, 2)], Table),
    r(IRON_PICK, 1, &[(IRON_INGOT, 3), (STICK, 2)], Table),
    r(DIAMOND_PICK, 1, &[(DIAMOND, 3), (STICK, 2)], Table),
    r(WOOD_SWORD, 1, &[(PLANKS, 2), (STICK, 1)], Table),
    r(STONE_SWORD, 1, &[(COBBLE, 2), (STICK, 1)], Table),
    r(IRON_SWORD, 1, &[(IRON_INGOT, 2), (STICK, 1)], Table),
    r(DIAMOND_SWORD, 1, &[(DIAMOND, 2), (STICK, 1)], Table),
    r(FURNACE, 1, &[(COBBLE, 8)], Table),
    r(BOOKSHELF, 1, &[(PLANKS, 6)], Table),
    r(STONE_BRICK, 4, &[(STONE, 4)], Table),
    r(SANDSTONE, 1, &[(SAND, 4)], Table),
    r(TNT, 1, &[(SAND, 4), (COAL, 5)], Table),
    r(IRON_BLOCK, 1, &[(IRON_INGOT, 9)], Table),
    r(GOLD_BLOCK, 1, &[(GOLD_INGOT, 9)], Table),
    r(DIAMOND_BLOCK, 1, &[(DIAMOND, 9)], Table),
    r(BREAD, 1, &[(TALL_GRASS, 3)], Table),
    r(STONE, 1, &[(COBBLE, 1), (COAL, 1)], Furnace),
    r(GLASS, 2, &[(SAND, 2), (COAL, 1)], Furnace),
    r(BRICK, 2, &[(CLAY, 2), (COAL, 1)], Furnace),
    r(IRON_INGOT, 1, &[(IRON_ORE, 1), (COAL, 1)], Furnace),
    r(GOLD_INGOT, 1, &[(GOLD_ORE, 1), (COAL, 1)], Furnace),
    r(GLOWSTONE, 1, &[(GOLD_INGOT, 1), (GLASS, 1)], Furnace),
];

impl Station {
    pub fn label(self) -> &'static str {
        match self {
            Hand => "",
            Table => "Crafting Table",
            Furnace => "Furnace",
        }
    }
    pub fn block(self) -> Option<BlockId> {
        match self {
            Hand => None,
            Table => Some(CRAFTING_TABLE),
            Furnace => Some(FURNACE),
        }
    }
}
