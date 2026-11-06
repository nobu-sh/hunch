//! Synthetic drum audio for tests.

use std::f32::consts::PI;

pub const SAMPLE_RATE: u32 = 22_050;

pub fn sample_count(seconds: f32) -> usize {
    (seconds * SAMPLE_RATE as f32) as usize
}

/// Deterministic white noise in [-1, 1].
struct Noise(u32);

impl Noise {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.0 >> 8) as f32 / (1u32 << 23) as f32 - 1.0
    }
}

fn kick() -> Vec<f32> {
    let mut phase = 0.0;
    (0..sample_count(0.3))
        .map(|i| {
            let t = i as f32 / SAMPLE_RATE as f32;
            phase += 2.0 * PI * (50.0 + 70.0 * (-t / 0.03).exp()) / SAMPLE_RATE as f32;
            0.9 * phase.sin() * (-t / 0.12).exp()
        })
        .collect()
}

fn hat() -> Vec<f32> {
    let mut noise = Noise(7);
    let mut previous = 0.0;
    (0..sample_count(0.05))
        .map(|i| {
            let t = i as f32 / SAMPLE_RATE as f32;
            let white = noise.next();
            // First difference tilts the noise toward the top octaves.
            let bright = white - previous;
            previous = white;
            0.25 * bright * (-t / 0.012).exp()
        })
        .collect()
}

/// A kick on every beat and a hat on every eighth note.
pub fn beat(bpm: f32, seconds: f32) -> Vec<f32> {
    let mut out = vec![0.0; sample_count(seconds)];
    let eighth_samples = 60.0 / bpm / 2.0 * SAMPLE_RATE as f32;
    let (kick, hat) = (kick(), hat());
    let mut add = |sound: &[f32], start: usize| {
        for (sample, value) in out.iter_mut().skip(start).zip(sound) {
            *sample += value;
        }
    };
    let eighths = (sample_count(seconds) as f32 / eighth_samples).ceil() as usize;
    for eighth in 0..eighths {
        let start = (eighth as f32 * eighth_samples).round() as usize;
        if eighth % 2 == 0 {
            add(&kick, start);
        }
        add(&hat, start);
    }
    out
}
