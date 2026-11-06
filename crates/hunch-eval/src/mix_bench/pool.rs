//! The tracks mixes are built from (Beatport val, keyed) and the seeded choice of pairs.

use std::path::{Path, PathBuf};

use hunch_core::Key;

use crate::SplitChoice;
use crate::keydata::{BEATPORT_DIR, KeyDataset};
use crate::metrics::is_same_scale_as_any;
use crate::timeline::{BEATPORT_META_DIR, store_bpm};

/// Tempi within this fraction of each other are the same tempo.
const SAME_TEMPO_TOLERANCE: f32 = 0.02;
/// Tempi at least this fraction apart, also from each other's half and double, are different.
const DIFFERENT_TEMPO_MIN: f32 = 0.06;
/// DJ pairs are resampled by at most this many semitones.
pub const MAX_DJ_SEMITONES: f32 = 1.0;
const OCTAVE_FACTORS: [f32; 3] = [1.0, 2.0, 0.5];

#[derive(Debug, Clone)]
pub struct PoolTrack {
    pub id: u64,
    pub audio_path: PathBuf,
    pub keys: Vec<Key>,
    /// Beatport's listed BPM, when its metadata has one.
    pub bpm: Option<f32>,
}

/// Keyed Beatport tracks of `split`, sorted by id.
pub fn load(data_dir: &Path, split: SplitChoice) -> Result<Vec<PoolTrack>, String> {
    let meta_dir = data_dir.join(BEATPORT_DIR).join(BEATPORT_META_DIR);
    let tracks = KeyDataset::load_beatport(data_dir)?.tracks;
    Ok(tracks
        .into_iter()
        .filter(|track| split.includes(track.split))
        .filter_map(|track| {
            let keys = track.label.clone().ok()?;
            Some(PoolTrack {
                id: track.id,
                bpm: store_bpm(&meta_dir, &track),
                audio_path: track.audio_path,
                keys,
            })
        })
        .collect())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Category {
    SameKeySameTempo,
    SameTempoDifferentKey,
    DifferentTempoSameKey,
    BothDifferent,
}

impl Category {
    pub const ALL: [Category; 4] = [
        Category::SameKeySameTempo,
        Category::SameTempoDifferentKey,
        Category::DifferentTempoSameKey,
        Category::BothDifferent,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Category::SameKeySameTempo => "same key+tempo",
            Category::SameTempoDifferentKey => "new key",
            Category::DifferentTempoSameKey => "new tempo",
            Category::BothDifferent => "new key+tempo",
        }
    }

    /// Key or tempo changes, so the readout needs a split.
    pub fn is_change(self) -> bool {
        self != Category::SameKeySameTempo
    }

    fn of(is_same_key: bool, is_same_tempo: bool) -> Self {
        match (is_same_key, is_same_tempo) {
            (true, true) => Category::SameKeySameTempo,
            (false, true) => Category::SameTempoDifferentKey,
            (true, false) => Category::DifferentTempoSameKey,
            (false, false) => Category::BothDifferent,
        }
    }
}

/// `Some(true)` for the same tempo, `Some(false)` for clearly different, `None` in between.
pub fn tempo_relation(a: f32, b: f32) -> Option<bool> {
    let ratio = a / b;
    if (ratio - 1.0).abs() <= SAME_TEMPO_TOLERANCE {
        return Some(true);
    }
    let nearest = OCTAVE_FACTORS
        .iter()
        .map(|factor| (ratio / factor - 1.0).abs())
        .fold(f32::INFINITY, f32::min);
    (nearest >= DIFFERENT_TEMPO_MIN).then_some(false)
}

pub fn is_same_key(a: &[Key], b: &[Key]) -> bool {
    a.iter().any(|&key| is_same_scale_as_any(key, b))
}

/// The category of a plain (not resampled) pair, or `None` if a tempo is missing or borderline.
pub fn plain_category(a: &PoolTrack, b: &PoolTrack) -> Option<Category> {
    let is_same_tempo = tempo_relation(a.bpm?, b.bpm?)?;
    Some(Category::of(is_same_key(&a.keys, &b.keys), is_same_tempo))
}

/// `keys` moved up by `semitones`.
pub fn transpose(keys: &[Key], semitones: i32) -> Vec<Key> {
    keys.iter()
        .map(|key| {
            let index = (i32::from(key.tonic.index()) + semitones).rem_euclid(12) as u8;
            Key {
                tonic: hunch_core::PitchClass::new(index).expect("index is in 0..12"),
                mode: key.mode,
            }
        })
        .collect()
}

/// SplitMix64: small, seedable and good enough for picking pairs.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `0.0..1.0`.
    pub fn unit(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    pub fn below(&mut self, count: usize) -> usize {
        (self.next_u64() % count.max(1) as u64) as usize
    }

    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for index in (1..items.len()).rev() {
            items.swap(index, self.below(index + 1));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tempo_relation_separates_same_borderline_and_different() {
        assert_eq!(tempo_relation(124.0, 124.0), Some(true));
        assert_eq!(tempo_relation(124.0, 126.0), Some(true));
        assert_eq!(tempo_relation(124.0, 128.0), None);
        assert_eq!(tempo_relation(120.0, 128.0), Some(false));
        assert_eq!(tempo_relation(70.0, 140.0), None);
    }

    #[test]
    fn rng_is_reproducible() {
        let draws = |seed| {
            let mut rng = Rng::new(seed);
            (0..4).map(|_| rng.next_u64()).collect::<Vec<_>>()
        };
        assert_eq!(draws(7), draws(7));
        assert_ne!(draws(7), draws(8));
    }
}
