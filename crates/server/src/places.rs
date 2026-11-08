//! Real places: looks a place up on OpenStreetMap, downloads its streets and buildings, and
//! builds them into the world far out east. Each one is saved with the world.
//!
//! Map data © OpenStreetMap contributors (ODbL). Lookups use the public Nominatim and Overpass
//! services, which ask for light use: one place at a time.

use crate::World;
use runecraft_common::osm::{self, Place};
use runecraft_common::protocol::{PlaceSummary, ToClient};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct PlaceInfo {
    pub name: String,
    pub file: String,
    pub arrival: [f32; 3],
}

impl PlaceInfo {
    pub fn arrival(&self) -> [f32; 3] {
        self.arrival
    }
}

/// A finished download: who asked, and the map data or what went wrong.
pub(crate) struct Loaded {
    conn: u32,
    result: Result<(String, Place), String>,
}

pub(crate) fn load(world_dir: &Path, info: &PlaceInfo) -> Option<Place> {
    let bytes = std::fs::read(world_dir.join("places").join(&info.file)).ok()?;
    bincode::deserialize(&bytes).ok()
}

fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(150))
        .user_agent(concat!("RuneCraft/", env!("CARGO_PKG_VERSION"), " (block-building game; github.com/RatikGambhir/helix-test)"))
        .try_proxy_from_env(true)
        .build()
}

fn read_all(r: ureq::Response) -> Result<String, String> {
    let mut s = String::new();
    r.into_reader().take(200 << 20).read_to_string(&mut s).map_err(|e| format!("download failed: {e}"))?;
    Ok(s)
}

/// "40.75, -73.98" or a name to look up.
fn locate(query: &str) -> Result<(f64, f64, String), String> {
    let nums: Vec<f64> = query.split([',', ' ']).filter(|s| !s.is_empty()).filter_map(|s| s.parse().ok()).collect();
    if nums.len() == 2 && (-90.0..=90.0).contains(&nums[0]) && (-180.0..=180.0).contains(&nums[1]) {
        return Ok((nums[0], nums[1], format!("{:.4}, {:.4}", nums[0], nums[1])));
    }
    let url = format!("https://nominatim.openstreetmap.org/search?format=json&limit=1&q={}", encode(query));
    let body = read_all(agent().get(&url).call().map_err(|e| format!("couldn't reach OpenStreetMap: {e}"))?)?;
    let v: serde_json::Value = serde_json::from_str(&body).map_err(|e| format!("odd answer from OpenStreetMap: {e}"))?;
    let hit = v.as_array().and_then(|a| a.first()).ok_or_else(|| format!("OpenStreetMap doesn't know \"{query}\""))?;
    let num = |k: &str| hit.get(k).and_then(|x| x.as_str()).and_then(|s| s.parse::<f64>().ok());
    let (lat, lon) = (num("lat").ok_or("no latitude")?, num("lon").ok_or("no longitude")?);
    let name = hit.get("display_name").and_then(|n| n.as_str()).unwrap_or(query).split(',').take(2).collect::<Vec<_>>().join(",");
    Ok((lat, lon, name.trim().to_string()))
}

fn fetch(query: &str) -> Result<(String, Place), String> {
    let (lat, lon, name) = locate(query)?;
    let q = osm::overpass_query(lat, lon, osm::HALF);
    let resp = agent()
        .post("https://overpass-api.de/api/interpreter")
        .send_form(&[("data", q.as_str())])
        .map_err(|e| format!("couldn't download the map: {e}"))?;
    let body = read_all(resp)?;
    let ways = osm::parse_overpass(&body)?;
    if ways.is_empty() {
        return Err(format!("OpenStreetMap has no streets or buildings mapped around {name}"));
    }
    Ok((name.clone(), osm::build_place(&name, &ways, lat, lon, osm::HALF, 0, 0)))
}

impl World {
    pub(crate) fn places_message(&self) -> ToClient {
        ToClient::Places(self.place_info.iter().map(|p| PlaceSummary { name: p.name.clone(), pos: p.arrival }).collect())
    }

    pub(crate) fn load_place(&mut self, conn: u32, query: String) {
        let query = query.trim().to_string();
        if query.is_empty() {
            return;
        }
        if let Some(i) = self.place_info.iter().position(|p| p.name.to_lowercase().contains(&query.to_lowercase())) {
            let pos = self.place_info[i].arrival;
            self.tell(conn, format!("{} is already built. Taking you there.", self.place_info[i].name));
            self.change_dim(conn, 0, pos);
            return;
        }
        self.tell(conn, format!("Downloading {query} from OpenStreetMap…"));
        let tx = self.place_results.0.clone();
        let _ = std::thread::Builder::new().name("osm".into()).spawn(move || {
            let result = fetch(&query);
            let _ = tx.send(Loaded { conn, result });
        });
    }

    /// Builds places whose downloads have finished.
    pub(crate) fn poll_places(&mut self) {
        while let Ok(Loaded { conn, result }) = self.place_results.1.try_recv() {
            match result {
                Ok((name, place)) => {
                    let pos = self.add_place(name.clone(), place);
                    self.system(format!("Built {name} from OpenStreetMap (map data © OpenStreetMap contributors). /places lists real places."));
                    let msg = self.places_message();
                    self.broadcast(&msg);
                    if self.players.contains_key(&conn) {
                        self.change_dim(conn, 0, pos);
                    }
                }
                Err(e) => self.tell(conn, e),
            }
        }
    }

    /// Puts a place in its slot, saves it, and makes the world rebuild that area.
    pub(crate) fn add_place(&mut self, name: String, mut place: Place) -> [f32; 3] {
        let n = self.place_info.len();
        let (cx, cz) = osm::slot(n);
        place.x0 = cx - place.w / 2;
        place.z0 = cz - place.h / 2;
        let file = format!("place-{n}.bin");
        let dir = self.cfg.world_dir.join("places");
        let _ = std::fs::create_dir_all(&dir);
        if let Ok(bytes) = bincode::serialize(&place) {
            let _ = std::fs::write(dir.join(&file), bytes);
        }
        let arrival = place.arrival();
        self.place_info.push(PlaceInfo { name, file, arrival });
        let place = Arc::new(place);
        if let Ok(mut list) = self.places.write() {
            list.push(place.clone());
        }
        // Any of it already generated is generated again with the place in it.
        self.enter(0);
        let stale: Vec<(i32, i32)> = self.lv.columns.keys().copied().filter(|&(x, z)| place.touches_chunk(x, z)).collect();
        for k in stale {
            self.lv.columns.remove(&k);
            for p in self.players.values_mut().filter(|p| p.dim == 0) {
                if p.sent.remove(&k) {
                    p.send(&ToClient::Unload { cx: k.0, cz: k.1 });
                }
            }
        }
        self.dirty = true;
        arrival
    }
}

#[cfg(test)]
mod tests {
    use crate::*;

    #[test]
    fn a_built_place_is_saved_and_comes_back() {
        let dir = std::env::temp_dir().join(format!("runecraft-places-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let json = r#"{"elements":[{"type":"way","id":7,"tags":{"highway":"residential"},"geometry":[{"lat":1.0,"lon":1.0},{"lat":1.0,"lon":1.002}]}]}"#;
        let ways = runecraft_common::osm::parse_overpass(json).unwrap();
        let place = runecraft_common::osm::build_place("Somewhere", &ways, 1.0, 1.001, 100.0, 0, 0);
        {
            let mut w = World::open(Config { world_dir: dir.clone(), ..Default::default() });
            let at = w.add_place("Somewhere".into(), place);
            assert!(at[0] > 50_000.0);
            w.save();
        }
        let w = World::open(Config { world_dir: dir.clone(), ..Default::default() });
        assert_eq!(w.place_info.len(), 1);
        assert_eq!(w.places.read().unwrap().len(), 1, "the map data loads with the world");
        let p = w.places.read().unwrap()[0].clone();
        let col = worldgen::generate_in(w.seed, 0, p.x0.div_euclid(16) + 6, p.z0.div_euclid(16) + 6, &[p.clone()]);
        assert!(col.blocks.iter().any(|&b| b == block::ASPHALT), "the street is built");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
