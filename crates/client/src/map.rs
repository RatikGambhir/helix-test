//! Top-down maps of the loaded world: a minimap in the corner and a big map on M.

use crate::game::State;
use runecraft_common::block::{self, BlockId};
use runecraft_common::chunk::{self, HEIGHT};
use std::sync::OnceLock;

/// The colour each block shows from above, averaged from its top texture.
fn colors(s: &State) -> &'static [[u8; 3]; 256] {
    static C: OnceLock<[[u8; 3]; 256]> = OnceLock::new();
    C.get_or_init(|| {
        let mut out = [[0u8; 3]; 256];
        for id in 1..block::BLOCK_COUNT {
            let tile = block::def(id as BlockId).tex[0];
            let (mut sum, mut n) = ([0u32; 3], 0u32);
            for y in 0..16 {
                for x in 0..16 {
                    let c = s.atlas.texel(tile, x, y);
                    if c[3] > 100 {
                        for k in 0..3 {
                            sum[k] += c[k] as u32;
                        }
                        n += 1;
                    }
                }
            }
            let n = n.max(1);
            out[id] = [(sum[0] / n) as u8, (sum[1] / n) as u8, (sum[2] / n) as u8];
        }
        out
    })
}

/// The topmost visible block of a world column, and its height.
fn top(s: &State, x: i32, z: i32) -> Option<(BlockId, i32)> {
    let ((cx, lx), (cz, lz)) = (chunk::split(x), chunk::split(z));
    let col = s.world.columns.get(&(cx, cz))?;
    let start = (col.height(lx, lz) + 48).min(HEIGHT - 1);
    let mut y = start;
    while y > 0 {
        let b = col.get(lx, y, lz);
        if b != block::AIR && b != block::TALL_GRASS {
            return Some((b, y));
        }
        y -= 1;
    }
    None
}

/// Paints `size`×`size` pixels, `bpp` blocks per pixel, centred on the player.
pub fn draw(s: &State, size: usize, bpp: i32, mini: bool) {
    let colors = colors(s);
    let (cx, cz) = (s.body.pos.x.floor() as i32, s.body.pos.z.floor() as i32);
    let half = size as i32 / 2;
    let mut rgba = vec![0u8; size * size * 4];
    let mut heights = vec![i32::MIN; size];
    for py in 0..size as i32 {
        for px in 0..size as i32 {
            let (wx, wz) = (cx + (px - half) * bpp, cz + (py - half) * bpp);
            let o = ((py as usize) * size + px as usize) * 4;
            let Some((b, h)) = top(s, wx, wz) else {
                rgba[o..o + 4].copy_from_slice(&[18, 20, 26, if mini { 200 } else { 255 }]);
                heights[px as usize] = i32::MIN;
                continue;
            };
            let mut c = colors[b as usize];
            if block::fluid(b).is_some_and(|f| f.0 == 0) {
                c = [52, 96, 200];
            }
            // Relief: lighter where the ground rises to the south.
            let north = heights[px as usize];
            let k = if north == i32::MIN { 1.0 } else if h > north { 1.18 } else if h < north { 0.82 } else { 1.0 };
            heights[px as usize] = h;
            rgba[o] = (c[0] as f32 * k).min(255.0) as u8;
            rgba[o + 1] = (c[1] as f32 * k).min(255.0) as u8;
            rgba[o + 2] = (c[2] as f32 * k).min(255.0) as u8;
            rgba[o + 3] = 255;
        }
    }
    // The player: a white arrow pointing where you look.
    let f = (s.yaw.sin(), -s.yaw.cos());
    for i in -4i32..=6 {
        for w in -3i32..=3 {
            let along = i as f32;
            let width = (6.0 - along) * 0.45;
            if (w as f32).abs() > width.max(0.0) || along < -1.0 {
                continue;
            }
            let x = half as f32 + f.0 * (along - 2.0) - f.1 * w as f32;
            let y = half as f32 + f.1 * (along - 2.0) + f.0 * w as f32;
            let (x, y) = (x.round() as i32, y.round() as i32);
            if (0..size as i32).contains(&x) && (0..size as i32).contains(&y) {
                let o = (y as usize * size + x as usize) * 4;
                rgba[o..o + 4].copy_from_slice(&[255, 255, 255, 255]);
            }
        }
    }
    let target = if mini { &s.minimap } else { &s.map };
    target.set(size as u32, size as u32, rgba);
}
