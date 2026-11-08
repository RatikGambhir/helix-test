//! Deterministic hashing and gradient noise, so every machine builds the same world from a seed.

#[inline]
pub fn hash(mut x: u64) -> u64 {
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51afd7ed558ccd);
    x ^= x >> 33;
    x = x.wrapping_mul(0xc4ceb9fe1a85ec53);
    x ^= x >> 33;
    x
}

#[inline]
pub fn hash2(seed: u64, x: i32, z: i32) -> u64 {
    hash(seed ^ (x as u32 as u64).wrapping_mul(0x9E3779B97F4A7C15) ^ (z as u32 as u64).wrapping_mul(0xC2B2AE3D27D4EB4F).rotate_left(31))
}

#[inline]
pub fn hash3(seed: u64, x: i32, y: i32, z: i32) -> u64 {
    hash(hash2(seed, x, z) ^ (y as u32 as u64).wrapping_mul(0x165667B19E3779F9))
}

/// A float in [0, 1) from a hash.
#[inline]
pub fn unit(h: u64) -> f32 {
    (h >> 40) as f32 / (1u64 << 24) as f32
}

#[inline]
fn fade(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

#[inline]
fn grad2(seed: u64, ix: i32, iz: i32, dx: f32, dz: f32) -> f32 {
    let a = unit(hash2(seed, ix, iz)) * std::f32::consts::TAU;
    a.cos() * dx + a.sin() * dz
}

/// 2D gradient noise in roughly [-1, 1].
pub fn perlin2(seed: u64, x: f32, z: f32) -> f32 {
    let (x0, z0) = (x.floor(), z.floor());
    let (ix, iz) = (x0 as i32, z0 as i32);
    let (fx, fz) = (x - x0, z - z0);
    let (u, v) = (fade(fx), fade(fz));
    let n00 = grad2(seed, ix, iz, fx, fz);
    let n10 = grad2(seed, ix + 1, iz, fx - 1.0, fz);
    let n01 = grad2(seed, ix, iz + 1, fx, fz - 1.0);
    let n11 = grad2(seed, ix + 1, iz + 1, fx - 1.0, fz - 1.0);
    let a = n00 + u * (n10 - n00);
    let b = n01 + u * (n11 - n01);
    (a + v * (b - a)) * 1.414
}

#[inline]
fn grad3(seed: u64, ix: i32, iy: i32, iz: i32, dx: f32, dy: f32, dz: f32) -> f32 {
    match hash3(seed, ix, iy, iz) & 15 {
        0 => dx + dy,
        1 => -dx + dy,
        2 => dx - dy,
        3 => -dx - dy,
        4 => dx + dz,
        5 => -dx + dz,
        6 => dx - dz,
        7 => -dx - dz,
        8 => dy + dz,
        9 => -dy + dz,
        10 => dy - dz,
        11 => -dy - dz,
        12 => dx + dy,
        13 => -dy + dz,
        14 => -dx + dy,
        _ => -dy - dz,
    }
}

/// 3D gradient noise in roughly [-1, 1].
pub fn perlin3(seed: u64, x: f32, y: f32, z: f32) -> f32 {
    let (x0, y0, z0) = (x.floor(), y.floor(), z.floor());
    let (ix, iy, iz) = (x0 as i32, y0 as i32, z0 as i32);
    let (fx, fy, fz) = (x - x0, y - y0, z - z0);
    let (u, v, w) = (fade(fx), fade(fy), fade(fz));
    let g = |ox: i32, oy: i32, oz: i32| grad3(seed, ix + ox, iy + oy, iz + oz, fx - ox as f32, fy - oy as f32, fz - oz as f32);
    let lerp = |a: f32, b: f32, t: f32| a + t * (b - a);
    let x00 = lerp(g(0, 0, 0), g(1, 0, 0), u);
    let x10 = lerp(g(0, 1, 0), g(1, 1, 0), u);
    let x01 = lerp(g(0, 0, 1), g(1, 0, 1), u);
    let x11 = lerp(g(0, 1, 1), g(1, 1, 1), u);
    lerp(lerp(x00, x10, v), lerp(x01, x11, v), w)
}

/// Fractal sum of `octaves` layers of 2D noise.
pub fn fbm2(seed: u64, x: f32, z: f32, octaves: u32) -> f32 {
    let (mut sum, mut amp, mut freq, mut norm) = (0.0, 1.0, 1.0, 0.0);
    for o in 0..octaves {
        sum += amp * perlin2(seed.wrapping_add(o as u64 * 7919), x * freq, z * freq);
        norm += amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    sum / norm
}
