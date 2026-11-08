//! Chunk columns: 16×16 blocks wide and `HEIGHT` tall, with run-length encoding for the wire.

use crate::block::{self, AIR, BlockId};

pub const CHUNK: i32 = 16;
pub const HEIGHT: i32 = 256;
pub const SECTIONS: i32 = HEIGHT / CHUNK;
pub const SEA_LEVEL: i32 = 62;
pub const COLUMN_VOLUME: usize = (CHUNK * CHUNK * HEIGHT) as usize;

#[derive(Clone)]
pub struct Column {
    pub blocks: Box<[BlockId]>,
    /// Highest opaque block per (x, z), -1 when none: sunlight stops there.
    pub heights: Box<[i16; 256]>,
}

impl Default for Column {
    fn default() -> Self {
        Column { blocks: vec![AIR; COLUMN_VOLUME].into_boxed_slice(), heights: Box::new([-1; 256]) }
    }
}

#[inline]
pub fn index(x: i32, y: i32, z: i32) -> usize {
    ((y * CHUNK + z) * CHUNK + x) as usize
}

/// The chunk a world coordinate falls in, and the coordinate within it.
#[inline]
pub fn split(v: i32) -> (i32, i32) {
    (v.div_euclid(CHUNK), v.rem_euclid(CHUNK))
}

impl Column {
    #[inline]
    pub fn get(&self, x: i32, y: i32, z: i32) -> BlockId {
        if !(0..HEIGHT).contains(&y) {
            return AIR;
        }
        self.blocks[index(x, y, z)]
    }

    #[inline]
    pub fn set_raw(&mut self, x: i32, y: i32, z: i32, b: BlockId) {
        if (0..HEIGHT).contains(&y) {
            self.blocks[index(x, y, z)] = b;
        }
    }

    pub fn set(&mut self, x: i32, y: i32, z: i32, b: BlockId) {
        if !(0..HEIGHT).contains(&y) {
            return;
        }
        self.blocks[index(x, y, z)] = b;
        let i = (z * CHUNK + x) as usize;
        let h = self.heights[i];
        if block::is_opaque(b) {
            if y as i16 > h {
                self.heights[i] = y as i16;
            }
        } else if y as i16 == h {
            self.heights[i] = self.scan_height(x, z, y - 1);
        }
    }

    fn scan_height(&self, x: i32, z: i32, from: i32) -> i16 {
        let mut y = from;
        while y >= 0 && !block::is_opaque(self.blocks[index(x, y, z)]) {
            y -= 1;
        }
        y as i16
    }

    pub fn recompute_heights(&mut self) {
        for z in 0..CHUNK {
            for x in 0..CHUNK {
                self.heights[(z * CHUNK + x) as usize] = self.scan_height(x, z, HEIGHT - 1);
            }
        }
    }

    #[inline]
    pub fn height(&self, x: i32, z: i32) -> i32 {
        self.heights[(z * CHUNK + x) as usize] as i32
    }

    /// Run-length encoded blocks: (count u16 LE, block) pairs.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(4096);
        let mut i = 0;
        let b = &self.blocks;
        while i < b.len() {
            let v = b[i];
            let mut n = 1;
            while i + n < b.len() && b[i + n] == v && n < u16::MAX as usize {
                n += 1;
            }
            out.extend_from_slice(&(n as u16).to_le_bytes());
            out.push(v);
            i += n;
        }
        out
    }

    pub fn decode(data: &[u8]) -> Option<Column> {
        let mut c = Column::default();
        let mut i = 0;
        for run in data.chunks_exact(3) {
            let n = u16::from_le_bytes([run[0], run[1]]) as usize;
            if i + n > COLUMN_VOLUME {
                return None;
            }
            c.blocks[i..i + n].fill(run[2]);
            i += n;
        }
        if i != COLUMN_VOLUME {
            return None;
        }
        c.recompute_heights();
        Some(c)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rle_round_trip_and_heights() {
        let mut c = Column::default();
        for x in 0..16 {
            for z in 0..16 {
                for y in 0..60 {
                    c.set(x, y, z, block::STONE);
                }
            }
        }
        c.set(3, 100, 4, block::GLASS);
        c.set(5, 90, 5, block::BRICK);
        let d = Column::decode(&c.encode()).unwrap();
        assert_eq!(d.get(3, 100, 4), block::GLASS);
        assert_eq!(d.height(3, 4), 59, "glass lets light through");
        assert_eq!(d.height(5, 5), 90);
        let mut d = d;
        d.set(5, 90, 5, AIR);
        assert_eq!(d.height(5, 5), 59);
    }
}
