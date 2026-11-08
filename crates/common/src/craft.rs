//! Crafting (shaped and shapeless, on 2×2 and 3×3 grids) and furnace smelting.

use crate::block::*;

/// What a pattern letter accepts.
#[derive(Clone, Copy, Debug)]
pub enum Want {
    Item(BlockId),
    AnyLog,
    AnyWool,
}

impl Want {
    pub fn accepts(self, id: BlockId) -> bool {
        match self {
            Want::Item(i) => i == id,
            Want::AnyLog => matches!(id, OAK_LOG | BIRCH_LOG | SPRUCE_LOG),
            Want::AnyWool => (WOOL_WHITE..=WOOL_BLACK).contains(&id),
        }
    }
    /// An item to show for this ingredient.
    pub fn example(self) -> BlockId {
        match self {
            Want::Item(i) => i,
            Want::AnyLog => OAK_LOG,
            Want::AnyWool => WOOL_WHITE,
        }
    }
}

#[derive(Debug)]
pub struct Recipe {
    pub out: BlockId,
    pub count: u8,
    /// Rows of the pattern; ' ' is empty. Shapeless recipes list their ingredients in one row.
    pub pattern: &'static [&'static str],
    pub key: &'static [(char, Want)],
    pub shapeless: bool,
}

impl Recipe {
    pub fn width(&self) -> usize {
        self.pattern.iter().map(|r| r.len()).max().unwrap_or(0)
    }
    pub fn height(&self) -> usize {
        self.pattern.len()
    }
    fn want(&self, c: char) -> Option<Want> {
        self.key.iter().find(|(k, _)| *k == c).map(|(_, w)| *w)
    }
    /// Fits in a 2×2 grid (craftable from the inventory).
    pub fn small(&self) -> bool {
        if self.shapeless {
            self.pattern[0].len() <= 4
        } else {
            self.width() <= 2 && self.height() <= 2
        }
    }
    /// Each ingredient and how many, for the recipe book.
    pub fn ingredients(&self) -> Vec<(Want, u8)> {
        let mut out: Vec<(char, Want, u8)> = Vec::new();
        for c in self.pattern.iter().flat_map(|r| r.chars()).filter(|c| *c != ' ') {
            match out.iter_mut().find(|(k, _, _)| *k == c) {
                Some(e) => e.2 += 1,
                None => out.push((c, self.want(c).unwrap_or(Want::Item(AIR)), 1)),
            }
        }
        out.into_iter().map(|(_, w, n)| (w, n)).collect()
    }
}

const fn shaped(out: BlockId, count: u8, pattern: &'static [&'static str], key: &'static [(char, Want)]) -> Recipe {
    Recipe { out, count, pattern, key, shapeless: false }
}
use Want::*;
const P: (char, Want) = ('P', Item(PLANKS));
const S: (char, Want) = ('S', Item(STICK));
const C: (char, Want) = ('C', Item(COBBLE));
const I: (char, Want) = ('I', Item(IRON_INGOT));
const D: (char, Want) = ('D', Item(DIAMOND));
const G: (char, Want) = ('G', Item(GOLD_INGOT));

macro_rules! tools {
    ($m:ident, $wood:expr, $stone:expr, $iron:expr, $dia:expr) => {
        [
            shaped($wood, 1, $m, &[('X', Item(PLANKS)), S]),
            shaped($stone, 1, $m, &[('X', Item(COBBLE)), S]),
            shaped($iron, 1, $m, &[('X', Item(IRON_INGOT)), S]),
            shaped($dia, 1, $m, &[('X', Item(DIAMOND)), S]),
        ]
    };
}

const PICK: &[&str] = &["XXX", " S ", " S "];
const AXE: &[&str] = &["XX", "XS", " S"];
const SHOVEL: &[&str] = &["X", "S", "S"];
const HOE: &[&str] = &["XX", " S", " S"];
const SWORD: &[&str] = &["X", "X", "S"];
const HELMET: &[&str] = &["XXX", "X X"];
const CHESTPLATE: &[&str] = &["X X", "XXX", "XXX"];
const LEGGINGS: &[&str] = &["XXX", "X X", "X X"];
const BOOTS: &[&str] = &["X X", "X X"];

pub static RECIPES: std::sync::LazyLock<Vec<Recipe>> = std::sync::LazyLock::new(|| {
    let mut r = vec![
        Recipe { out: PLANKS, count: 4, pattern: &["L"], key: &[('L', AnyLog)], shapeless: true },
        shaped(STICK, 4, &["P", "P"], &[P]),
        shaped(CRAFTING_TABLE, 1, &["PP", "PP"], &[P]),
        shaped(TORCH, 4, &["K", "S"], &[('K', Item(COAL)), S]),
        shaped(CHEST, 1, &["PPP", "P P", "PPP"], &[P]),
        shaped(FURNACE, 1, &["CCC", "C C", "CCC"], &[C]),
        shaped(DOOR, 3, &["PP", "PP", "PP"], &[P]),
        shaped(LADDER, 3, &["S S", "SSS", "S S"], &[S]),
        shaped(BED, 1, &["WWW", "PPP"], &[('W', AnyWool), P]),
        shaped(BOW, 1, &[" TS", "T S", " TS"], &[('T', Item(STICK)), ('S', Item(STRING))]),
        shaped(ARROW, 4, &["F", "S", "E"], &[('F', Item(FLINT)), S, ('E', Item(FEATHER))]),
        shaped(BUCKET, 1, &["I I", " I "], &[I]),
        shaped(BREAD, 1, &["WWW"], &[('W', Item(WHEAT))]),
        shaped(TNT, 1, &["SKS", "KSK", "SKS"], &[('S', Item(SAND)), ('K', Item(COAL))]),
        shaped(LANTERN, 1, &[" I ", "ITI", " I "], &[I, ('T', Item(TORCH))]),
        shaped(STONE_BRICK, 4, &["SS", "SS"], &[('S', Item(STONE))]),
        shaped(SANDSTONE, 1, &["SS", "SS"], &[('S', Item(SAND))]),
        shaped(BOOKSHELF, 1, &["PPP", "LLL", "PPP"], &[P, ('L', Item(LEATHER))]),
        shaped(WOOL_WHITE, 1, &["SS", "SS"], &[('S', Item(STRING))]),
        shaped(RAIL, 16, &["I I", "ISI", "I I"], &[I, S]),
        shaped(IRON_BLOCK, 1, &["III", "III", "III"], &[I]),
        shaped(GOLD_BLOCK, 1, &["GGG", "GGG", "GGG"], &[G]),
        shaped(DIAMOND_BLOCK, 1, &["DDD", "DDD", "DDD"], &[D]),
        shaped(COPPER, 1, &["CCC", "CCC", "CCC"], &[('C', Item(COPPER_INGOT))]),
        shaped(GLOWSTONE, 1, &["GT"], &[G, ('T', Item(TORCH))]),
        shaped(GLASS, 1, &["G"], &[('G', Item(WINDOW_BLUE))]),
        Recipe { out: IRON_INGOT, count: 9, pattern: &["B"], key: &[('B', Item(IRON_BLOCK))], shapeless: true },
        Recipe { out: GOLD_INGOT, count: 9, pattern: &["B"], key: &[('B', Item(GOLD_BLOCK))], shapeless: true },
        Recipe { out: DIAMOND, count: 9, pattern: &["B"], key: &[('B', Item(DIAMOND_BLOCK))], shapeless: true },
        Recipe { out: BONE_MEAL, count: 3, pattern: &["B"], key: &[('B', Item(BONE))], shapeless: true },
    ];
    r.extend(tools!(PICK, WOOD_PICK, STONE_PICK, IRON_PICK, DIAMOND_PICK));
    r.extend(tools!(AXE, WOOD_AXE, WOOD_AXE + 1, WOOD_AXE + 2, DIAMOND_AXE));
    r.extend(tools!(SHOVEL, WOOD_SHOVEL, WOOD_SHOVEL + 1, WOOD_SHOVEL + 2, DIAMOND_SHOVEL));
    r.extend(tools!(HOE, WOOD_HOE, WOOD_HOE + 1, WOOD_HOE + 2, DIAMOND_HOE));
    r.extend(tools!(SWORD, WOOD_SWORD, STONE_SWORD, IRON_SWORD, DIAMOND_SWORD));
    for (pat, iron, dia) in [(HELMET, IRON_HELMET, DIAMOND_HELMET), (CHESTPLATE, IRON_CHESTPLATE, DIAMOND_CHESTPLATE), (LEGGINGS, IRON_LEGGINGS, DIAMOND_LEGGINGS), (BOOTS, IRON_BOOTS, DIAMOND_BOOTS)] {
        r.push(shaped(iron, 1, pat, &[('X', Item(IRON_INGOT))]));
        r.push(shaped(dia, 1, pat, &[('X', Item(DIAMOND))]));
    }
    r
});

/// The recipe a crafting grid (row-major, `size`×`size`) makes, if any.
pub fn match_grid(grid: &[Option<BlockId>], size: usize) -> Option<&'static Recipe> {
    let cells: Vec<(usize, usize, BlockId)> = grid.iter().enumerate().filter_map(|(i, c)| c.map(|id| (i % size, i / size, id))).collect();
    if cells.is_empty() {
        return None;
    }
    let (x0, x1) = (cells.iter().map(|c| c.0).min()?, cells.iter().map(|c| c.0).max()?);
    let (y0, y1) = (cells.iter().map(|c| c.1).min()?, cells.iter().map(|c| c.1).max()?);
    let (w, h) = (x1 - x0 + 1, y1 - y0 + 1);
    let at = |x: usize, y: usize| grid[(y0 + y) * size + x0 + x];
    RECIPES.iter().find(|r| {
        if r.shapeless {
            let mut need: Vec<char> = r.pattern[0].chars().collect();
            if need.len() != cells.len() {
                return false;
            }
            for &(_, _, id) in &cells {
                let Some(i) = need.iter().position(|c| r.want(*c).is_some_and(|w| w.accepts(id))) else { return false };
                need.remove(i);
            }
            return true;
        }
        if r.width() != w || r.height() != h {
            return false;
        }
        let fits = |mirror: bool| {
            (0..h).all(|y| {
                (0..w).all(|x| {
                    let row = r.pattern[y].as_bytes();
                    let px = if mirror { w - 1 - x } else { x };
                    let c = row.get(px).copied().unwrap_or(b' ') as char;
                    match (c, at(x, y)) {
                        (' ', None) => true,
                        (' ', Some(_)) | (_, None) => false,
                        (c, Some(id)) => r.want(c).is_some_and(|wnt| wnt.accepts(id)),
                    }
                })
            })
        };
        fits(false) || fits(true)
    })
}

/// What a furnace turns an item into.
pub fn smelt(id: BlockId) -> Option<BlockId> {
    Some(match id {
        IRON_ORE => IRON_INGOT,
        GOLD_ORE => GOLD_INGOT,
        COPPER_ORE => COPPER_INGOT,
        SAND => GLASS,
        COBBLE => STONE,
        CLAY => BRICK,
        OAK_LOG | BIRCH_LOG | SPRUCE_LOG => COAL,
        RAW_PORK => COOKED_PORK,
        RAW_BEEF => STEAK,
        RAW_CHICKEN => COOKED_CHICKEN,
        RAW_MUTTON => COOKED_MUTTON,
        CACTUS => WOOL_GREEN,
        STONE => STONE_BRICK,
        _ => return None,
    })
}

/// Seconds to smelt one item.
pub const SMELT_TIME: f32 = 6.0;

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(cells: &[Option<BlockId>]) -> Option<BlockId> {
        let size = if cells.len() == 4 { 2 } else { 3 };
        match_grid(cells, size).map(|r| r.out)
    }

    #[test]
    fn shaped_recipes_match_anywhere_and_mirrored() {
        let p = Some(PLANKS);
        let s = Some(STICK);
        let n = None;
        assert_eq!(grid(&[p, n, p, n]), Some(STICK));
        assert_eq!(grid(&[n, p, n, p]), Some(STICK), "shifted");
        assert_eq!(grid(&[p, p, p, p]), Some(CRAFTING_TABLE));
        assert_eq!(grid(&[p, p, p, n, s, n, n, s, n]), Some(WOOD_PICK));
        assert_eq!(grid(&[p, p, n, s, p, n, s, n, n]), Some(WOOD_AXE), "mirrored axe");
        assert_eq!(grid(&[Some(BIRCH_LOG), n, n, n]), Some(PLANKS), "any log, shapeless");
        assert_eq!(grid(&[p, s, n, n]), None);
    }
}
