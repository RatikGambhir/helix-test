//! Turns 16³ sections into vertex lists on worker threads, with flood-filled sky and block
//! light, smooth per-vertex lighting and ambient occlusion.

use bytemuck::{Pod, Zeroable};
use cubeterra_common::block::{self, AIR, BlockId, Kind, WATER};
use cubeterra_common::chunk::{CHUNK, Column, HEIGHT};
use std::collections::VecDeque;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock};

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub uv: [f32; 2],
    /// Sky light, block light, shade (direction × AO), flags.
    pub light: [u8; 4],
}

pub type SectionKey = (i32, i32, i32);

pub struct Job {
    pub key: SectionKey,
    pub version: u32,
    /// 3×3 columns around the section's column, index (dz + 1) * 3 + (dx + 1).
    pub cols: [Option<Arc<Column>>; 9],
}

pub struct Output {
    pub key: SectionKey,
    pub version: u32,
    pub opaque: Vec<Vertex>,
    pub water: Vec<Vertex>,
}

pub struct Mesher {
    jobs: Sender<Job>,
    pub results: Receiver<Output>,
    pub in_flight: usize,
    pub workers: usize,
}

impl Mesher {
    pub fn new() -> Mesher {
        let (jobs, rx) = mpsc::channel::<Job>();
        let (tx, results) = mpsc::channel();
        let rx = Arc::new(Mutex::new(rx));
        let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).saturating_sub(1).clamp(1, 6);
        for i in 0..workers {
            let rx = rx.clone();
            let tx: Sender<Output> = tx.clone();
            let _ = std::thread::Builder::new().name(format!("mesh-{i}")).spawn(move || {
                loop {
                    let job = rx.lock().unwrap().recv();
                    let Ok(job) = job else { return };
                    let out = build(&job);
                    if tx.send(out).is_err() {
                        return;
                    }
                }
            });
        }
        Mesher { jobs, results, in_flight: 0, workers }
    }

    pub fn submit(&mut self, job: Job) {
        if self.jobs.send(job).is_ok() {
            self.in_flight += 1;
        }
    }
}

const P: i32 = 8;
const S: i32 = CHUNK + 2 * P;

struct Region<'a> {
    cols: &'a [Option<Arc<Column>>; 9],
    y0: i32,
}

impl Region<'_> {
    /// Block at section-local coordinates; x and z may reach into the neighbours.
    #[inline]
    fn get(&self, x: i32, y: i32, z: i32) -> BlockId {
        let wy = self.y0 + y;
        if !(0..HEIGHT).contains(&wy) {
            return AIR;
        }
        let (cx, lx) = (x.div_euclid(CHUNK), x.rem_euclid(CHUNK));
        let (cz, lz) = (z.div_euclid(CHUNK), z.rem_euclid(CHUNK));
        match &self.cols[((cz + 1) * 3 + cx + 1) as usize] {
            Some(c) => c.blocks[((wy * CHUNK + lz) * CHUNK + lx) as usize],
            None => AIR,
        }
    }
    #[inline]
    fn height(&self, x: i32, z: i32) -> i32 {
        let (cx, lx) = (x.div_euclid(CHUNK), x.rem_euclid(CHUNK));
        let (cz, lz) = (z.div_euclid(CHUNK), z.rem_euclid(CHUNK));
        match &self.cols[((cz + 1) * 3 + cx + 1) as usize] {
            Some(c) => c.height(lx, lz),
            None => -1,
        }
    }
}

#[inline]
fn li(x: i32, y: i32, z: i32) -> usize {
    (((y + P) * S + (z + P)) * S + (x + P)) as usize
}

#[inline]
fn in_region(x: i32, y: i32, z: i32) -> bool {
    (-P..CHUNK + P).contains(&x) && (-P..CHUNK + P).contains(&y) && (-P..CHUNK + P).contains(&z)
}

/// Sky and block light for the section and a margin around it.
struct Light {
    sky: Vec<u8>,
    blk: Vec<u8>,
    opaque: Vec<bool>,
}

fn flood(level: &mut [u8], opaque: &[bool], mut queue: VecDeque<(i32, i32, i32)>) {
    while let Some((x, y, z)) = queue.pop_front() {
        let v = level[li(x, y, z)];
        if v <= 1 {
            continue;
        }
        for (dx, dy, dz) in [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)] {
            let (nx, ny, nz) = (x + dx, y + dy, z + dz);
            if !in_region(nx, ny, nz) {
                continue;
            }
            let i = li(nx, ny, nz);
            if opaque[i] || level[i] >= v - 1 {
                continue;
            }
            level[i] = v - 1;
            queue.push_back((nx, ny, nz));
        }
    }
}

fn compute_light(r: &Region) -> Light {
    let n = (S * S * S) as usize;
    let mut sky = vec![0u8; n];
    let mut blk = vec![0u8; n];
    let mut opaque = vec![false; n];
    let mut bq = VecDeque::new();
    for z in -P..CHUNK + P {
        for x in -P..CHUNK + P {
            let h = r.height(x, z) - r.y0;
            for y in -P..CHUNK + P {
                let i = li(x, y, z);
                let b = r.get(x, y, z);
                let d = block::def(b);
                opaque[i] = d.kind == Kind::Solid;
                if y > h {
                    sky[i] = 15;
                }
                if d.light > 0 {
                    blk[i] = d.light;
                    bq.push_back((x, y, z));
                }
            }
        }
    }
    // Only lit cells next to darker open cells need to spread.
    let mut sq = VecDeque::new();
    for z in -P..CHUNK + P {
        for x in -P..CHUNK + P {
            for y in -P..CHUNK + P {
                let i = li(x, y, z);
                if sky[i] != 15 {
                    continue;
                }
                let edge = [(1, 0, 0), (-1, 0, 0), (0, 0, 1), (0, 0, -1), (0, -1, 0)].iter().any(|&(dx, dy, dz)| {
                    let (nx, ny, nz) = (x + dx, y + dy, z + dz);
                    in_region(nx, ny, nz) && {
                        let j = li(nx, ny, nz);
                        !opaque[j] && sky[j] < 15
                    }
                });
                if edge {
                    sq.push_back((x, y, z));
                }
            }
        }
    }
    flood(&mut sky, &opaque, sq);
    flood(&mut blk, &opaque, bq);
    Light { sky, blk, opaque }
}

/// Face directions: +X, -X, +Y, -Y, +Z, -Z.
const DIRS: [[i32; 3]; 6] = [[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1]];
const SHADE: [f32; 6] = [0.62, 0.62, 1.0, 0.5, 0.8, 0.8];

/// Corner positions of each face, counter-clockwise from outside.
fn faces() -> &'static [[[i32; 3]; 4]; 6] {
    static F: OnceLock<[[[i32; 3]; 4]; 6]> = OnceLock::new();
    F.get_or_init(|| {
        let mut f = [
            [[1, 0, 0], [1, 1, 0], [1, 1, 1], [1, 0, 1]],
            [[0, 0, 0], [0, 0, 1], [0, 1, 1], [0, 1, 0]],
            [[0, 1, 0], [0, 1, 1], [1, 1, 1], [1, 1, 0]],
            [[0, 0, 0], [1, 0, 0], [1, 0, 1], [0, 0, 1]],
            [[0, 0, 1], [1, 0, 1], [1, 1, 1], [0, 1, 1]],
            [[0, 0, 0], [0, 1, 0], [1, 1, 0], [1, 0, 0]],
        ];
        for (d, face) in f.iter_mut().enumerate() {
            let v = |i: usize| glam::Vec3::new(face[i][0] as f32, face[i][1] as f32, face[i][2] as f32);
            let n = (v(1) - v(0)).cross(v(2) - v(0));
            let want = glam::Vec3::new(DIRS[d][0] as f32, DIRS[d][1] as f32, DIRS[d][2] as f32);
            if n.dot(want) < 0.0 {
                face.reverse();
            }
        }
        f
    })
}

pub fn tile_uv(tile: u8, u: f32, v: f32) -> [f32; 2] {
    let (tx, ty) = ((tile % 16) as f32, (tile / 16) as f32);
    let e = 0.0008;
    [(tx + e + u * (1.0 - 2.0 * e)) / 16.0, (ty + e + v * (1.0 - 2.0 * e)) / 16.0]
}

fn visible(cur: BlockId, nb: BlockId) -> bool {
    let (c, n) = (block::def(cur).kind, block::def(nb).kind);
    match c {
        Kind::Solid => n != Kind::Solid,
        Kind::Cutout => n != Kind::Solid && !(cur == nb && cur == block::GLASS),
        Kind::Liquid => matches!(n, Kind::Air | Kind::Cutout | Kind::Plant) && nb != cur,
        _ => false,
    }
}

fn build(job: &Job) -> Output {
    let (cx, sy, cz) = job.key;
    let region = Region { cols: &job.cols, y0: sy * CHUNK };
    let mut out = Output { key: job.key, version: job.version, opaque: Vec::new(), water: Vec::new() };
    // Quick reject: an all-air section has nothing to draw.
    let Some(own) = &job.cols[4] else { return out };
    let start = (sy * CHUNK * CHUNK * CHUNK) as usize;
    if own.blocks[start..start + 4096].iter().all(|&b| b == AIR) {
        return out;
    }
    let mut light: Option<Light> = None;
    let (ox, oy, oz) = ((cx * CHUNK) as f32, (sy * CHUNK) as f32, (cz * CHUNK) as f32);
    let faces = faces();
    for y in 0..CHUNK {
        for z in 0..CHUNK {
            for x in 0..CHUNK {
                let b = region.get(x, y, z);
                if b == AIR {
                    continue;
                }
                let def = block::def(b);
                if def.kind == Kind::Plant {
                    let l = light.get_or_insert_with(|| compute_light(&region));
                    let i = li(x, y, z);
                    let lt = [l.sky[i] * 17, l.blk[i] * 17, 230, 1];
                    let tile = def.tex[0];
                    let p = [x as f32 + ox, y as f32 + oy, z as f32 + oz];
                    for (a, c) in [([0.15, 0.15], [0.85, 0.85]), ([0.15, 0.85], [0.85, 0.15])] {
                        let quad = [
                            ([a[0], 0.0, a[1]], [0.0, 1.0]),
                            ([c[0], 0.0, c[1]], [1.0, 1.0]),
                            ([c[0], 1.0, c[1]], [1.0, 0.0]),
                            ([a[0], 1.0, a[1]], [0.0, 0.0]),
                        ];
                        let verts: Vec<Vertex> = quad
                            .iter()
                            .map(|(q, uv)| Vertex { pos: [p[0] + q[0], p[1] + q[1], p[2] + q[2]], uv: tile_uv(tile, uv[0], uv[1]), light: lt })
                            .collect();
                        out.opaque.extend_from_slice(&verts);
                        out.opaque.extend([verts[0], verts[3], verts[2], verts[1]]);
                    }
                    continue;
                }
                let water = def.kind == Kind::Liquid && b == WATER;
                let water_top = def.kind == Kind::Liquid && !block::is_liquid(region.get(x, y + 1, z));
                for d in 0..6 {
                    let n = DIRS[d];
                    let nb = region.get(x + n[0], y + n[1], z + n[2]);
                    if !visible(b, nb) {
                        continue;
                    }
                    let l = light.get_or_insert_with(|| compute_light(&region));
                    let tile = match d {
                        2 => def.tex[0],
                        3 => def.tex[2],
                        _ => def.tex[1],
                    };
                    let q = [x + n[0], y + n[1], z + n[2]];
                    let axes: [usize; 2] = match d / 2 {
                        0 => [1, 2],
                        1 => [0, 2],
                        _ => [0, 1],
                    };
                    let mut verts = [Vertex { pos: [0.0; 3], uv: [0.0; 2], light: [0; 4] }; 4];
                    let mut aos = [0u8; 4];
                    for (k, c) in faces[d].iter().enumerate() {
                        let mut side = [[0i32; 3]; 2];
                        for (s, &a) in axes.iter().enumerate() {
                            side[s][a] = c[a] * 2 - 1;
                        }
                        let s1 = [q[0] + side[0][0], q[1] + side[0][1], q[2] + side[0][2]];
                        let s2 = [q[0] + side[1][0], q[1] + side[1][1], q[2] + side[1][2]];
                        let co = [s1[0] + side[1][0], s1[1] + side[1][1], s1[2] + side[1][2]];
                        let op = |p: [i32; 3]| in_region(p[0], p[1], p[2]) && l.opaque[li(p[0], p[1], p[2])];
                        let (o1, o2, oc) = (op(s1), op(s2), op(co));
                        let ao = if o1 && o2 { 0 } else { 3 - (o1 as u8 + o2 as u8 + oc as u8) };
                        aos[k] = ao;
                        // Smooth light: average the open cells around this corner.
                        let (mut sk, mut bl, mut cnt) = (0u32, 0u32, 0u32);
                        for (p, blocked) in [(q, false), (s1, o1), (s2, o2), (co, oc || (o1 && o2))] {
                            if blocked || !in_region(p[0], p[1], p[2]) {
                                continue;
                            }
                            let i = li(p[0], p[1], p[2]);
                            if l.opaque[i] && p != q {
                                continue;
                            }
                            sk += l.sky[i] as u32;
                            bl += l.blk[i] as u32;
                            cnt += 1;
                        }
                        let cnt = cnt.max(1);
                        let (sk, bl) = ((sk * 17 / cnt) as u8, (bl * 17 / cnt) as u8);
                        let ao_k = [0.48, 0.64, 0.82, 1.0][ao as usize];
                        let shade = (SHADE[d] * if def.kind == Kind::Liquid { 1.0 } else { ao_k } * 255.0) as u8;
                        let mut pos = [(x + c[0]) as f32 + ox, (y + c[1]) as f32 + oy, (z + c[2]) as f32 + oz];
                        if water_top && c[1] == 1 {
                            pos[1] -= 0.12;
                        }
                        let (u, v) = match d / 2 {
                            0 => (c[2] as f32, 1.0 - c[1] as f32),
                            1 => (c[0] as f32, c[2] as f32),
                            _ => (c[0] as f32, 1.0 - c[1] as f32),
                        };
                        let flags = if water { 2 } else if def.kind == Kind::Cutout { 1 } else { 0 };
                        verts[k] = Vertex { pos, uv: tile_uv(tile, u, v), light: [sk, bl, shade, flags] };
                    }
                    // Flip the quad's diagonal so AO gradients don't crease.
                    if aos[0] as u32 + aos[2] as u32 > aos[1] as u32 + aos[3] as u32 {
                        verts.rotate_left(1);
                    }
                    if water {
                        out.water.extend_from_slice(&verts);
                        if d == 2 {
                            // Seen from underwater too.
                            out.water.extend([verts[0], verts[3], verts[2], verts[1]]);
                        }
                    } else {
                        out.opaque.extend_from_slice(&verts);
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lone_block_makes_six_lit_faces() {
        let mut c = Column::default();
        c.set(5, 70, 5, block::STONE);
        let col = Arc::new(c);
        let cols: [Option<Arc<Column>>; 9] = std::array::from_fn(|_| Some(col.clone()));
        let out = build(&Job { key: (0, 4, 0), version: 0, cols });
        assert_eq!(out.opaque.len(), 24);
        assert!(out.opaque.iter().all(|v| v.light[0] > 200), "open sky lights every face");
    }
}
