//! Real places from OpenStreetMap: turns map data (roads, buildings, water, parks, railways)
//! into a grid of cells, one block per metre, and builds world columns from it.
//!
//! Map data © OpenStreetMap contributors, available under the Open Database License.

use crate::block::*;
use crate::noise::{hash2, unit};
use serde::{Deserialize, Serialize};

/// Ground level of every real place.
pub const GROUND: i32 = 64;
/// Blocks around a place where it blends into the land.
pub const MARGIN: i32 = 24;
/// Metres from the centre to each edge of a downloaded place.
pub const HALF: f32 = 300.0;

pub mod kind {
    pub const LAND: u8 = 0;
    pub const ROAD: u8 = 1;
    pub const ROAD_LINE: u8 = 2;
    pub const FOOTWAY: u8 = 3;
    pub const WATER: u8 = 4;
    pub const PARK: u8 = 5;
    pub const BUILDING: u8 = 6;
    pub const RAILWAY: u8 = 7;
    pub const PLAZA: u8 = 8;
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Cell {
    pub kind: u8,
    /// Building height in blocks.
    pub height: u8,
    /// Building look (wall material).
    pub style: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Place {
    pub name: String,
    /// World x, z of cell (0, 0).
    pub x0: i32,
    pub z0: i32,
    pub w: i32,
    pub h: i32,
    pub cells: Vec<Cell>,
}

impl Place {
    pub fn cell(&self, x: i32, z: i32) -> Cell {
        let (u, v) = (x - self.x0, z - self.z0);
        if u < 0 || v < 0 || u >= self.w || v >= self.h {
            return Cell::default();
        }
        self.cells[(v * self.w + u) as usize]
    }

    pub fn contains(&self, x: i32, z: i32) -> bool {
        (self.x0..self.x0 + self.w).contains(&x) && (self.z0..self.z0 + self.h).contains(&z)
    }

    /// Inside the place or its blending margin.
    pub fn near(&self, x: i32, z: i32) -> bool {
        (self.x0 - MARGIN..self.x0 + self.w + MARGIN).contains(&x) && (self.z0 - MARGIN..self.z0 + self.h + MARGIN).contains(&z)
    }

    pub fn touches_chunk(&self, cx: i32, cz: i32) -> bool {
        let (x, z) = (cx * 16, cz * 16);
        x + 16 > self.x0 - MARGIN && x < self.x0 + self.w + MARGIN && z + 16 > self.z0 - MARGIN && z < self.z0 + self.h + MARGIN
    }

    /// 0 inside, growing to 1 at the outer edge of the margin.
    pub fn blend(&self, x: i32, z: i32) -> f32 {
        let dx = (self.x0 - x).max(x - (self.x0 + self.w - 1)).max(0);
        let dz = (self.z0 - z).max(z - (self.z0 + self.h - 1)).max(0);
        (dx.max(dz) as f32 / MARGIN as f32).min(1.0)
    }

    /// A good place to arrive: the road cell nearest the centre.
    pub fn arrival(&self) -> [f32; 3] {
        let (cu, cv) = (self.w / 2, self.h / 2);
        let mut best = (i32::MAX, cu, cv);
        for v in 0..self.h {
            for u in 0..self.w {
                let c = self.cells[(v * self.w + u) as usize];
                if matches!(c.kind, kind::ROAD | kind::FOOTWAY | kind::PLAZA) {
                    let d = (u - cu).pow(2) + (v - cv).pow(2);
                    if d < best.0 {
                        best = (d, u, v);
                    }
                }
            }
        }
        [(self.x0 + best.1) as f32 + 0.5, GROUND as f32 + 1.0, (self.z0 + best.2) as f32 + 0.5]
    }

    fn is_trunk(&self, x: i32, z: i32) -> bool {
        // One tree per 6×6 patch of park, at a hashed spot, well inside the park.
        let (gx, gz) = (x.div_euclid(6), z.div_euclid(6));
        let h = hash2(0x7AEE, gx, gz);
        let (tx, tz) = (gx * 6 + 1 + (h % 4) as i32, gz * 6 + 1 + ((h >> 8) % 4) as i32);
        tx == x && tz == z && unit(h >> 16) < 0.7 && (-1..=1).all(|d| self.cell(x + d, z).kind == kind::PARK && self.cell(x, z + d).kind == kind::PARK)
    }

    /// Builds one world column from the place (x, z inside `near`). Clears what was there.
    pub fn fill_column(&self, seed: u64, x: i32, z: i32, natural: i32, put: &mut dyn FnMut(i32, BlockId)) {
        let g = GROUND;
        for y in 0..crate::chunk::HEIGHT {
            put(y, AIR);
        }
        if !self.contains(x, z) {
            // The margin: grass blending from the place's ground to the natural land.
            let t = self.blend(x, z);
            let h = (g as f32 + (natural - g) as f32 * t * t).round() as i32;
            for y in 0..=h {
                put(y, if y == 0 { BEDROCK } else if y < h - 3 { STONE } else if y < h { DIRT } else { GRASS });
            }
            for y in (h + 1)..crate::chunk::SEA_LEVEL {
                put(y, WATER);
            }
            return;
        }
        let c = self.cell(x, z);
        put(0, BEDROCK);
        for y in 1..(g - 3) {
            put(y, STONE);
        }
        for y in (g - 3)..g {
            put(y, DIRT);
        }
        let r = unit(hash2(seed ^ 0x05A, x, z));
        match c.kind {
            kind::ROAD => put(g, ASPHALT),
            kind::ROAD_LINE => put(g, ROAD_LINE),
            kind::FOOTWAY | kind::PLAZA => {
                put(g, SIDEWALK);
                if c.kind == kind::FOOTWAY && r < 0.012 && (-1..=1).any(|d| self.cell(x + d, z).kind == kind::ROAD || self.cell(x, z + d).kind == kind::ROAD) {
                    for y in 1..=3 {
                        put(g + y, STEEL);
                    }
                    put(g + 4, LANTERN);
                }
            }
            kind::WATER => {
                for y in (g - 4)..=g {
                    put(y, WATER);
                }
                put(g - 5, SAND);
            }
            kind::RAILWAY => {
                put(g, GRAVEL);
                let rail = |dx: i32, dz: i32| self.cell(x + dx, z + dz).kind == kind::RAILWAY;
                put(g + 1, if (rail(1, 0) || rail(-1, 0)) && !(rail(0, 1) || rail(0, -1)) { RAIL_EW } else { RAIL });
            }
            kind::BUILDING => self.building(c, x, z, r, put),
            kind::PARK => {
                put(g, GRASS);
                if self.is_trunk(x, z) {
                    for y in 1..=4 {
                        put(g + y, OAK_LOG);
                    }
                    put(g + 5, OAK_LEAVES);
                    put(g + 6, OAK_LEAVES);
                } else if (-1..=1).any(|dx| (-1..=1).any(|dz| self.is_trunk(x + dx, z + dz))) {
                    put(g + 4, OAK_LEAVES);
                    put(g + 5, OAK_LEAVES);
                } else if r < 0.1 {
                    put(g + 1, TALL_GRASS);
                } else if r < 0.12 {
                    put(g + 1, FLOWER_RED);
                }
            }
            _ => put(g, GRASS),
        }
    }

    fn building(&self, c: Cell, x: i32, z: i32, r: f32, put: &mut dyn FnMut(i32, BlockId)) {
        let g = GROUND;
        let h = c.height.max(4) as i32;
        // Walls where the neighbour is lower (or not a building).
        let side = |dx: i32, dz: i32| {
            let n = self.cell(x + dx, z + dz);
            if n.kind == kind::BUILDING { n.height.max(4) as i32 } else { 0 }
        };
        let lowest = side(1, 0).min(side(-1, 0)).min(side(0, 1)).min(side(0, -1));
        let tall = h > 40;
        let (wall, window) = match (c.style % 6, tall) {
            (_, true) if c.style % 2 == 0 => (STEEL, WINDOW_BLUE),
            (_, true) => (CONCRETE_DARK, WINDOW_DARK),
            (0, _) => (BRICK, WINDOW_WARM),
            (1, _) => (CONCRETE_WHITE, WINDOW_BLUE),
            (2, _) => (LIMESTONE, WINDOW_DARK),
            (3, _) => (CONCRETE_GRAY, WINDOW_WARM),
            (4, _) => (TERRACOTTA, WINDOW_DARK),
            _ => (SANDSTONE, WINDOW_WARM),
        };
        put(g, CONCRETE_GRAY);
        // A doorway where the ground floor faces a street.
        let street = [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|&(dx, dz)| matches!(self.cell(x + dx, z + dz).kind, kind::FOOTWAY | kind::ROAD | kind::PLAZA));
        let door = street && r < 0.12;
        for y in 1..=h {
            let level = y % 4;
            let blk = if y == h {
                if tall { CONCRETE_GRAY } else if c.style % 3 == 0 { SLATE } else { CONCRETE_GRAY }
            } else if y <= lowest {
                // Inside: a floor every four blocks.
                if level == 0 { PLANKS } else { AIR }
            } else if door && y <= 2 {
                AIR
            } else if level == 0 {
                wall
            } else if (level == 2 || level == 3 && tall) && (x + z) % 2 == 0 || tall && level != 0 {
                window
            } else {
                wall
            };
            put(g + y, blk);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Map data

/// A way from an Overpass `out geom` answer: its tags and points (lat, lon).
#[derive(Debug, Default)]
pub struct Way {
    pub id: u64,
    pub tags: Vec<(String, String)>,
    pub points: Vec<(f64, f64)>,
}

impl Way {
    pub fn tag(&self, k: &str) -> Option<&str> {
        self.tags.iter().find(|(a, _)| a == k).map(|(_, v)| v.as_str())
    }
    fn closed(&self) -> bool {
        self.points.len() > 3 && self.points.first() == self.points.last()
    }
}

/// Reads the ways (and the outer rings of multipolygon relations) from Overpass JSON.
pub fn parse_overpass(json: &str) -> Result<Vec<Way>, String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| format!("map data unreadable: {e}"))?;
    let mut out = Vec::new();
    let elements = v.get("elements").and_then(|e| e.as_array()).ok_or("map data has no elements")?;
    let points = |g: &serde_json::Value| -> Vec<(f64, f64)> {
        g.as_array()
            .map(|a| a.iter().filter_map(|p| Some((p.get("lat")?.as_f64()?, p.get("lon")?.as_f64()?))).collect())
            .unwrap_or_default()
    };
    for e in elements {
        let tags: Vec<(String, String)> = e
            .get("tags")
            .and_then(|t| t.as_object())
            .map(|t| t.iter().filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string()))).collect())
            .unwrap_or_default();
        let id = e.get("id").and_then(|i| i.as_u64()).unwrap_or(0);
        match e.get("type").and_then(|t| t.as_str()) {
            Some("way") => {
                if let Some(g) = e.get("geometry") {
                    out.push(Way { id, tags, points: points(g) });
                }
            }
            Some("relation") => {
                for m in e.get("members").and_then(|m| m.as_array()).into_iter().flatten() {
                    if m.get("role").and_then(|r| r.as_str()) == Some("outer")
                        && let Some(g) = m.get("geometry")
                    {
                        out.push(Way { id, tags: tags.clone(), points: points(g) });
                    }
                }
            }
            _ => {}
        }
    }
    Ok(out)
}

/// The Overpass query for everything within `half` metres of a point.
pub fn overpass_query(lat: f64, lon: f64, half: f32) -> String {
    let (s, w, n, e) = bbox(lat, lon, half);
    let b = format!("{s:.6},{w:.6},{n:.6},{e:.6}");
    format!(
        "[out:json][timeout:90];(way[\"building\"]({b});way[\"highway\"]({b});way[\"railway\"=\"rail\"]({b});way[\"railway\"=\"subway\"]({b});way[\"natural\"=\"water\"]({b});way[\"waterway\"]({b});way[\"leisure\"=\"park\"]({b});way[\"landuse\"=\"grass\"]({b});way[\"leisure\"=\"garden\"]({b});relation[\"natural\"=\"water\"]({b});relation[\"leisure\"=\"park\"]({b});relation[\"building\"]({b}););out geom;"
    )
}

fn bbox(lat: f64, lon: f64, half: f32) -> (f64, f64, f64, f64) {
    let dlat = half as f64 / 110_540.0;
    let dlon = half as f64 / (111_320.0 * lat.to_radians().cos().max(0.05));
    (lat - dlat, lon - dlon, lat + dlat, lon + dlon)
}

fn levels_height(w: &Way) -> u8 {
    let num = |s: &str| s.trim().split(|c: char| !(c.is_ascii_digit() || c == '.')).next().and_then(|n| n.parse::<f32>().ok());
    let blocks = if let Some(h) = w.tag("height").and_then(num) {
        h
    } else if let Some(l) = w.tag("building:levels").and_then(num) {
        l * 4.0
    } else {
        let levels = match w.tag("building").unwrap_or("yes") {
            "house" | "detached" | "semidetached_house" | "bungalow" | "garage" | "garages" | "shed" | "hut" => 2.0,
            "apartments" | "residential" => 5.0,
            "commercial" | "office" | "hotel" => 7.0,
            "retail" | "supermarket" | "warehouse" | "industrial" => 2.0,
            "church" | "cathedral" | "train_station" => 5.0,
            _ => 3.0,
        };
        levels * 4.0
    };
    (blocks.round() as i32).clamp(4, 180) as u8
}

/// Builds a place `half` metres each way around (lat, lon), with its cell (0, 0) at world
/// (x0, z0).
pub fn build_place(name: &str, ways: &[Way], lat: f64, lon: f64, half: f32, x0: i32, z0: i32) -> Place {
    let size = (half * 2.0) as i32;
    let mut cells = vec![Cell::default(); (size * size) as usize];
    let k = lat.to_radians().cos();
    // Metres east and south of the north-west corner.
    let to_grid = |p: (f64, f64)| -> (f32, f32) {
        let east = ((p.1 - lon) * 111_320.0 * k) as f32 + half;
        let south = ((lat - p.0) * 110_540.0) as f32 + half;
        (east, south)
    };
    let paint_poly = |cells: &mut Vec<Cell>, pts: &[(f32, f32)], cell: Cell, only_land: bool| {
        if pts.len() < 3 {
            return;
        }
        let (mut minv, mut maxv) = (f32::MAX, f32::MIN);
        for p in pts {
            minv = minv.min(p.1);
            maxv = maxv.max(p.1);
        }
        let (v0, v1) = ((minv.floor() as i32).max(0), (maxv.ceil() as i32).min(size - 1));
        for v in v0..=v1 {
            // Scanline: where the edges cross the row's middle.
            let y = v as f32 + 0.5;
            let mut xs: Vec<f32> = Vec::new();
            for i in 0..pts.len() {
                let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
                if (a.1 <= y) != (b.1 <= y) {
                    xs.push(a.0 + (y - a.1) / (b.1 - a.1) * (b.0 - a.0));
                }
            }
            xs.sort_by(|a, b| a.total_cmp(b));
            for pair in xs.chunks_exact(2) {
                let (u0, u1) = ((pair[0] - 0.5).ceil().max(0.0) as i32, ((pair[1] - 0.5).floor() as i32).min(size - 1));
                for u in u0..=u1 {
                    let c = &mut cells[(v * size + u) as usize];
                    if !only_land || c.kind == kind::LAND {
                        *c = cell;
                    }
                }
            }
        }
    };
    let paint_line = |cells: &mut Vec<Cell>, pts: &[(f32, f32)], width: f32, cell: Cell, centre: Option<Cell>| {
        for seg in pts.windows(2) {
            let (a, b) = (seg[0], seg[1]);
            let r = width / 2.0;
            let (u0, u1) = ((a.0.min(b.0) - r).floor().max(0.0) as i32, ((a.0.max(b.0) + r).ceil() as i32).min(size - 1));
            let (v0, v1) = ((a.1.min(b.1) - r).floor().max(0.0) as i32, ((a.1.max(b.1) + r).ceil() as i32).min(size - 1));
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let len2 = (dx * dx + dy * dy).max(1e-6);
            for v in v0..=v1 {
                for u in u0..=u1 {
                    let (px, py) = (u as f32 + 0.5, v as f32 + 0.5);
                    let t = (((px - a.0) * dx + (py - a.1) * dy) / len2).clamp(0.0, 1.0);
                    let (cx, cy) = (a.0 + dx * t, a.1 + dy * t);
                    let d = ((px - cx).powi(2) + (py - cy).powi(2)).sqrt();
                    if d <= r {
                        let c = &mut cells[(v * size + u) as usize];
                        if c.kind == kind::BUILDING {
                            continue;
                        }
                        *c = match centre {
                            Some(cc) if d < 0.6 => cc,
                            _ => cell,
                        };
                    }
                }
            }
        }
    };
    let grid = |w: &Way| -> Vec<(f32, f32)> { w.points.iter().map(|p| to_grid(*p)).collect() };
    let at = |kind: u8| Cell { kind, height: 0, style: 0 };
    // Land cover first, then water, roads, railways and finally buildings on top.
    for w in ways {
        if (w.tag("leisure").is_some_and(|l| l == "park" || l == "garden") || w.tag("landuse") == Some("grass")) && w.closed() {
            paint_poly(&mut cells, &grid(w), at(kind::PARK), false);
        }
        if w.tag("highway").is_some_and(|h| h == "pedestrian") && w.closed() && w.tag("area") == Some("yes") {
            paint_poly(&mut cells, &grid(w), at(kind::PLAZA), false);
        }
    }
    for w in ways {
        if w.tag("natural") == Some("water") && w.closed() {
            paint_poly(&mut cells, &grid(w), at(kind::WATER), false);
        }
        if let Some(ww) = w.tag("waterway") {
            let width = match ww {
                "river" => 16.0,
                "canal" => 8.0,
                _ => 3.0,
            };
            paint_line(&mut cells, &grid(w), width, at(kind::WATER), None);
        }
    }
    for w in ways {
        let Some(hw) = w.tag("highway") else { continue };
        if w.tag("area") == Some("yes") || w.tag("tunnel").is_some_and(|t| t != "no") {
            continue;
        }
        let (width, kind_, line) = match hw {
            "motorway" | "trunk" => (16.0, kind::ROAD, true),
            "primary" => (12.0, kind::ROAD, true),
            "secondary" => (10.0, kind::ROAD, true),
            "tertiary" => (8.0, kind::ROAD, true),
            "residential" | "unclassified" | "living_street" => (7.0, kind::ROAD, false),
            "service" => (4.0, kind::ROAD, false),
            "pedestrian" => (6.0, kind::PLAZA, false),
            "footway" | "path" | "cycleway" | "steps" => (2.0, kind::FOOTWAY, false),
            _ => continue,
        };
        // Sidewalks along the roads that carry traffic.
        if kind_ == kind::ROAD && width >= 7.0 {
            paint_line(&mut cells, &grid(w), width + 4.0, at(kind::FOOTWAY), None);
        }
        paint_line(&mut cells, &grid(w), width, at(kind_), if line { Some(at(kind::ROAD_LINE)) } else { None });
    }
    for w in ways {
        if w.tag("railway").is_some_and(|r| r == "rail") && w.tag("tunnel").is_none_or(|t| t == "no") {
            paint_line(&mut cells, &grid(w), 1.0, at(kind::RAILWAY), None);
        }
    }
    for w in ways {
        if w.tag("building").is_some() && w.closed() {
            let cell = Cell { kind: kind::BUILDING, height: levels_height(w), style: (w.id % 251) as u8 };
            paint_poly(&mut cells, &grid(w), cell, false);
        }
    }
    Place { name: name.to_string(), x0, z0, w: size, h: size, cells }
}

/// Where the `n`th place loaded into a world goes: far out east, away from everything else.
pub fn slot(n: usize) -> (i32, i32) {
    let size = (HALF * 2.0) as i32 + 2 * MARGIN + 200;
    (60_000 + n as i32 * size, 60_000)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{"elements":[
      {"type":"way","id":1,"tags":{"highway":"primary","name":"Main"},
       "geometry":[{"lat":40.7500,"lon":-73.9900},{"lat":40.7500,"lon":-73.9860}]},
      {"type":"way","id":2,"tags":{"building":"yes","building:levels":"10"},
       "geometry":[{"lat":40.7503,"lon":-73.9885},{"lat":40.7503,"lon":-73.9878},{"lat":40.7508,"lon":-73.9878},{"lat":40.7508,"lon":-73.9885},{"lat":40.7503,"lon":-73.9885}]},
      {"type":"way","id":3,"tags":{"leisure":"park"},
       "geometry":[{"lat":40.7490,"lon":-73.9890},{"lat":40.7490,"lon":-73.9870},{"lat":40.7480,"lon":-73.9870},{"lat":40.7480,"lon":-73.9890},{"lat":40.7490,"lon":-73.9890}]}
    ]}"#;

    #[test]
    fn map_data_becomes_streets_and_buildings() {
        let ways = parse_overpass(SAMPLE).unwrap();
        assert_eq!(ways.len(), 3);
        let p = build_place("Test", &ways, 40.75, -73.988, 200.0, 1000, 2000);
        let count = |k: u8| p.cells.iter().filter(|c| c.kind == k).count();
        assert!(count(kind::ROAD) > 1000, "the avenue is painted");
        assert!(count(kind::ROAD_LINE) > 100, "with a centre line");
        assert!(count(kind::BUILDING) > 2000, "the building has a footprint");
        assert!(count(kind::PARK) > 5000, "and the park");
        let b = p.cells.iter().find(|c| c.kind == kind::BUILDING).unwrap();
        assert_eq!(b.height, 40, "ten floors, four blocks each");
        // A column through the building has walls or windows up to its roof.
        let (bx, bz) = (0..p.h).flat_map(|v| (0..p.w).map(move |u| (u, v))).find(|&(u, v)| p.cells[(v * p.w + u) as usize].kind == kind::BUILDING).unwrap();
        let mut col = vec![AIR; 256];
        p.fill_column(1, p.x0 + bx, p.z0 + bz, 70, &mut |y, b| col[y as usize] = b);
        assert_ne!(col[(GROUND + 40) as usize], AIR, "a roof on top");
        let a = p.arrival();
        assert!(matches!(p.cell(a[0] as i32, a[2] as i32).kind, kind::ROAD | kind::FOOTWAY), "you arrive on the street");
    }
}
