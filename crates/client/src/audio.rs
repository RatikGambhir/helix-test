//! Sound effects and music, synthesised on the fly: there are no audio files. A background
//! thread owns the output device; the game sends it sounds to play.

use std::sync::mpsc::{self, Sender};
use std::time::{Duration, Instant};

const RATE: u32 = 22050;

#[derive(Clone, Copy, Debug)]
pub enum Sound {
    /// Breaking a block of a material: 0 stone, 1 wood, 2 soil, 3 glass, 4 sand/snow, 5 plant.
    Break(u8),
    /// The tick of mining in progress.
    Dig(u8),
    Place(u8),
    Step(u8),
    Hurt,
    Pickup,
    Explosion,
    Thunder,
    Splash,
    Click,
    Bow,
    Hit,
    Eat,
    Door,
    Growl,
    Moo,
    Oink,
    Cluck,
    Baa,
}

pub struct Audio {
    tx: Option<Sender<(Sound, f32)>>,
    music: Option<Sender<bool>>,
}

impl Audio {
    pub fn new() -> Audio {
        if std::env::var_os("RUNECRAFT_MUTE").is_some() {
            return Audio { tx: None, music: None };
        }
        let (tx, rx) = mpsc::channel::<(Sound, f32)>();
        let (mtx, mrx) = mpsc::channel::<bool>();
        let ok = std::thread::Builder::new()
            .name("audio".into())
            .spawn(move || {
                let Ok((_stream, handle)) = rodio::OutputStream::try_default() else { return };
                let mut seed = 0x1234_5678u32;
                let mut music_on = true;
                let mut next_song = Instant::now() + Duration::from_secs(20);
                loop {
                    while let Ok(on) = mrx.try_recv() {
                        music_on = on;
                    }
                    match rx.recv_timeout(Duration::from_millis(250)) {
                        Ok((s, vol)) => {
                            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                            let samples = synth(s, vol, seed);
                            let _ = handle.play_raw(rodio::buffer::SamplesBuffer::new(1, RATE, samples));
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                        Err(mpsc::RecvTimeoutError::Disconnected) => return,
                    }
                    if music_on && Instant::now() >= next_song {
                        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                        let _ = handle.play_raw(rodio::buffer::SamplesBuffer::new(1, RATE, song(seed)));
                        next_song = Instant::now() + Duration::from_secs(90 + (seed % 120) as u64);
                    }
                }
            })
            .is_ok();
        if ok { Audio { tx: Some(tx), music: Some(mtx) } } else { Audio { tx: None, music: None } }
    }

    pub fn play(&self, s: Sound, volume: f32) {
        if volume > 0.01
            && let Some(tx) = &self.tx
        {
            let _ = tx.send((s, volume.min(1.0)));
        }
    }

    pub fn set_music(&self, on: bool) {
        if let Some(m) = &self.music {
            let _ = m.send(on);
        }
    }
}

struct Rng(u32);
impl Rng {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

/// Filtered noise with an attack/decay envelope.
fn noise(len: f32, cutoff: f32, decay: f32, rng: &mut Rng) -> Vec<f32> {
    let n = (len * RATE as f32) as usize;
    let k = (cutoff / RATE as f32 * std::f32::consts::TAU).min(1.0);
    let mut lp = 0.0;
    (0..n)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            lp += (rng.next() - lp) * k;
            let env = (t * 400.0).min(1.0) * (-t * decay).exp();
            lp * env
        })
        .collect()
}

fn tone(len: f32, f0: f32, f1: f32, decay: f32, harmonics: &[f32]) -> Vec<f32> {
    let n = (len * RATE as f32) as usize;
    let mut phase = 0.0f32;
    (0..n)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let f = f0 + (f1 - f0) * (t / len);
            phase += f / RATE as f32 * std::f32::consts::TAU;
            let s: f32 = harmonics.iter().enumerate().map(|(h, a)| a * (phase * (h + 1) as f32).sin()).sum();
            s * (t * 300.0).min(1.0) * (-t * decay).exp()
        })
        .collect()
}

fn mix(a: Vec<f32>, b: Vec<f32>, gb: f32) -> Vec<f32> {
    let n = a.len().max(b.len());
    (0..n).map(|i| a.get(i).copied().unwrap_or(0.0) + b.get(i).copied().unwrap_or(0.0) * gb).collect()
}

fn material(m: u8) -> (f32, f32) {
    // (cutoff Hz, decay)
    match m {
        0 => (3200.0, 22.0),
        1 => (900.0, 26.0),
        2 => (500.0, 18.0),
        3 => (6000.0, 14.0),
        4 => (2500.0, 16.0),
        _ => (1800.0, 30.0),
    }
}

fn synth(s: Sound, vol: f32, seed: u32) -> Vec<f32> {
    let mut rng = Rng(seed | 1);
    let mut out = match s {
        Sound::Break(m) => {
            let (c, d) = material(m);
            let mut v = noise(0.28, c, d * 0.6, &mut rng);
            if m == 3 {
                v = mix(v, tone(0.3, 2400.0, 2300.0, 12.0, &[0.3, 0.2]), 1.0);
            }
            v
        }
        Sound::Dig(m) => {
            let (c, d) = material(m);
            noise(0.08, c, d * 1.5, &mut rng).into_iter().map(|x| x * 0.6).collect()
        }
        Sound::Place(m) => {
            let (c, _) = material(m);
            mix(noise(0.12, c * 0.7, 35.0, &mut rng), tone(0.12, 140.0, 90.0, 30.0, &[0.5]), 1.0)
        }
        Sound::Step(m) => {
            let (c, _) = material(m);
            noise(0.07, c * 0.6, 50.0, &mut rng).into_iter().map(|x| x * 0.35).collect()
        }
        Sound::Hurt => mix(tone(0.25, 420.0, 180.0, 10.0, &[0.6, 0.3, 0.15]), noise(0.2, 1500.0, 20.0, &mut rng), 0.4),
        Sound::Pickup => {
            let a = tone(0.07, 880.0, 990.0, 30.0, &[0.4]);
            let mut b = vec![0.0; (0.06 * RATE as f32) as usize];
            b.extend(tone(0.09, 1320.0, 1400.0, 30.0, &[0.35]));
            mix(a, b, 1.0)
        }
        Sound::Explosion => noise(1.4, 400.0, 3.0, &mut rng).into_iter().map(|x| x * 2.2).collect(),
        Sound::Thunder => {
            let base = noise(3.0, 180.0, 1.0, &mut rng);
            let crack = noise(0.4, 3000.0, 9.0, &mut rng);
            mix(base.into_iter().map(|x| x * 2.5).collect(), crack, 0.8)
        }
        Sound::Splash => noise(0.5, 2200.0, 7.0, &mut rng),
        Sound::Click => tone(0.03, 1800.0, 1600.0, 120.0, &[0.4]),
        Sound::Bow => mix(tone(0.25, 260.0, 200.0, 14.0, &[0.5, 0.25]), noise(0.15, 4000.0, 25.0, &mut rng), 0.5),
        Sound::Hit => mix(noise(0.12, 1200.0, 30.0, &mut rng), tone(0.1, 200.0, 120.0, 30.0, &[0.6]), 0.8),
        Sound::Eat => {
            let mut v = Vec::new();
            for _ in 0..3 {
                v.extend(noise(0.09, 1600.0, 35.0, &mut rng));
                v.extend(vec![0.0; (0.06 * RATE as f32) as usize]);
            }
            v
        }
        Sound::Door => mix(noise(0.15, 700.0, 25.0, &mut rng), tone(0.18, 180.0, 140.0, 20.0, &[0.5, 0.2]), 1.0),
        Sound::Growl => tone(0.7, 95.0, 80.0, 3.0, &[0.5, 0.35, 0.25, 0.15]).into_iter().enumerate().map(|(i, x)| x * (1.0 + 0.4 * (i as f32 / 900.0).sin())).collect(),
        Sound::Moo => tone(0.8, 140.0, 110.0, 2.5, &[0.5, 0.3, 0.2]),
        Sound::Oink => tone(0.25, 300.0, 240.0, 8.0, &[0.4, 0.3, 0.2]),
        Sound::Cluck => tone(0.08, 900.0, 700.0, 30.0, &[0.4, 0.2]),
        Sound::Baa => tone(0.5, 380.0, 340.0, 4.0, &[0.4, 0.3, 0.2, 0.1]),
    };
    for x in &mut out {
        *x = (*x * vol * 0.6).clamp(-1.0, 1.0);
    }
    out
}

/// A short, calm piano-like phrase on a pentatonic scale.
fn song(seed: u32) -> Vec<f32> {
    let mut rng = Rng(seed | 1);
    let scale = [0, 2, 4, 7, 9, 12, 14, 16];
    let root = [220.0, 196.0, 246.94, 174.61][(seed % 4) as usize];
    let notes = 10 + (seed % 8) as usize;
    let beat = 0.6 + (seed % 3) as f32 * 0.15;
    let len = notes as f32 * beat + 3.0;
    let mut out = vec![0.0f32; (len * RATE as f32) as usize];
    let mut step = 3usize;
    for i in 0..notes {
        let r = (rng.next() * 2.0).round() as i32;
        step = (step as i32 + r).clamp(0, scale.len() as i32 - 1) as usize;
        let f = root * 2f32.powf(scale[step] as f32 / 12.0);
        let note = tone(2.6, f, f, 1.6, &[0.22, 0.08, 0.04]);
        let start = (i as f32 * beat * RATE as f32) as usize;
        for (k, s) in note.into_iter().enumerate() {
            if let Some(o) = out.get_mut(start + k) {
                *o += s;
            }
        }
        if i % 4 == 0 {
            let bass = tone(3.0, root / 2.0, root / 2.0, 1.0, &[0.12]);
            for (k, s) in bass.into_iter().enumerate() {
                if let Some(o) = out.get_mut(start + k) {
                    *o += s;
                }
            }
        }
    }
    out.into_iter().map(|x| x * 0.5).collect()
}
