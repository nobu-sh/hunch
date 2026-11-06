//! Key scoring on summed chroma blocks, the way `hunch-core` ranks keys (Pearson correlation with
//! the default profile plus the bass tonic cue), without its presence gate or key hold.

use hunch_core::Mode;
use hunch_core::tonal::{ChromaBlock, KeyDetectorConfig};

pub const PITCH_CLASSES: usize = 12;
pub const KEY_COUNT: usize = 24;
const MODES: [Mode; 2] = [Mode::Major, Mode::Minor];
const RELATIVE_MINOR_OFFSET: usize = 9;

pub type Chroma = [f32; PITCH_CLASSES];

/// Blocks summed over a stretch of ticks.
#[derive(Debug, Clone, Copy, Default)]
pub struct Evidence {
    pub treble: Chroma,
    pub bass: Chroma,
    pub tonal_seconds: f32,
}

impl Evidence {
    pub fn from_block(block: &ChromaBlock) -> Self {
        Self {
            treble: block.treble,
            bass: block.bass,
            tonal_seconds: block.tonal_seconds,
        }
    }

    pub fn plus(&self, other: &Self) -> Self {
        Self {
            treble: std::array::from_fn(|i| self.treble[i] + other.treble[i]),
            bass: std::array::from_fn(|i| self.bass[i] + other.bass[i]),
            tonal_seconds: self.tonal_seconds + other.tonal_seconds,
        }
    }

    pub fn minus(&self, other: &Self) -> Self {
        Self {
            treble: std::array::from_fn(|i| (self.treble[i] - other.treble[i]).max(0.0)),
            bass: std::array::from_fn(|i| (self.bass[i] - other.bass[i]).max(0.0)),
            tonal_seconds: (self.tonal_seconds - other.tonal_seconds).max(0.0),
        }
    }
}

/// Running sums so the evidence of any tick range is one subtraction.
pub struct PrefixEvidence {
    sums: Vec<Evidence>,
}

impl PrefixEvidence {
    pub fn new(blocks: impl Iterator<Item = Evidence>) -> Self {
        let mut sums = vec![Evidence::default()];
        for block in blocks {
            let next = sums[sums.len() - 1].plus(&block);
            sums.push(next);
        }
        Self { sums }
    }

    /// Evidence of ticks `from..to`.
    pub fn range(&self, from: usize, to: usize) -> Evidence {
        let to = to.min(self.sums.len() - 1);
        let from = from.min(to);
        self.sums[to].minus(&self.sums[from])
    }
}

pub struct Scorer {
    /// Centered, unit-norm templates per key index (`mode * 12 + tonic`).
    templates: [[f32; PITCH_CLASSES]; KEY_COUNT],
    tonic_weight: f32,
}

impl Scorer {
    pub fn new() -> Self {
        let config = KeyDetectorConfig::default();
        let templates = std::array::from_fn(|index| {
            let mode = MODES[index / PITCH_CLASSES];
            let tonic = index % PITCH_CLASSES;
            let template = config.profile.template(mode);
            let rotated: Chroma = std::array::from_fn(|pitch| {
                template[(pitch + PITCH_CLASSES - tonic) % PITCH_CLASSES]
            });
            centered_unit(&rotated).expect("profiles are not flat")
        });
        Self {
            templates,
            tonic_weight: config.tonic_weight,
        }
    }

    /// Every key's score, or `None` when the chroma is flat.
    pub fn scores(&self, evidence: &Evidence) -> Option<[f32; KEY_COUNT]> {
        let chroma = centered_unit(&evidence.treble)?;
        let bass_max = evidence.bass.iter().copied().fold(0.0, f32::max);
        Some(std::array::from_fn(|index| {
            let correlation: f32 = chroma
                .iter()
                .zip(&self.templates[index])
                .map(|(a, b)| a * b)
                .sum();
            let tonic_cue = if bass_max > 0.0 {
                evidence.bass[index % PITCH_CLASSES] / bass_max
            } else {
                0.0
            };
            correlation + self.tonic_weight * tonic_cue
        }))
    }
}

pub fn best(scores: &[f32; KEY_COUNT]) -> usize {
    (0..KEY_COUNT)
        .max_by(|&a, &b| scores[a].total_cmp(&scores[b]))
        .unwrap_or(0)
}

/// The key with the same notes (relative major/minor).
pub fn relative(index: usize) -> usize {
    let tonic = index % PITCH_CLASSES;
    if index < PITCH_CLASSES {
        PITCH_CLASSES + (tonic + RELATIVE_MINOR_OFFSET) % PITCH_CLASSES
    } else {
        (tonic + PITCH_CLASSES - RELATIVE_MINOR_OFFSET) % PITCH_CLASSES
    }
}

/// How far `key`'s scale trails the best key on `scores`: 0 when it (or its relative) is best.
pub fn scale_gap(scores: &[f32; KEY_COUNT], key: usize) -> f32 {
    let top = scores[best(scores)];
    top - scores[key].max(scores[relative(key)])
}

/// How far the best key beats the best key outside its scale.
pub fn separation(scores: &[f32; KEY_COUNT]) -> f32 {
    let top = best(scores);
    let rival = (0..KEY_COUNT)
        .filter(|&index| index != top && index != relative(top))
        .map(|index| scores[index])
        .fold(f32::NEG_INFINITY, f32::max);
    scores[top] - rival
}

pub fn centered_unit(values: &[f32; PITCH_CLASSES]) -> Option<[f32; PITCH_CLASSES]> {
    let mean = values.iter().sum::<f32>() / PITCH_CLASSES as f32;
    let centered = values.map(|value| value - mean);
    let norm = centered
        .iter()
        .map(|value| value * value)
        .sum::<f32>()
        .sqrt();
    (norm > 1e-9).then(|| centered.map(|value| value / norm))
}

/// Pearson correlation of two chroma vectors; 0 when either is flat.
pub fn correlation(a: &Chroma, b: &Chroma) -> f32 {
    match (centered_unit(a), centered_unit(b)) {
        (Some(a), Some(b)) => a.iter().zip(&b).map(|(x, y)| x * y).sum(),
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_keys_pair_up() {
        // C major (0) and A minor (12 + 9).
        assert_eq!(relative(0), 21);
        assert_eq!(relative(21), 0);
        for index in 0..KEY_COUNT {
            assert_eq!(relative(relative(index)), index);
        }
    }

    #[test]
    fn c_major_chroma_scores_c_major_or_a_minor_best() {
        let mut treble = [0.0; PITCH_CLASSES];
        for pitch in [0, 2, 4, 5, 7, 9, 11] {
            treble[pitch] = 1.0;
        }
        treble[0] = 2.0;
        treble[7] = 1.5;
        let scores = Scorer::new()
            .scores(&Evidence {
                treble,
                bass: [0.0; 12],
                tonal_seconds: 1.0,
            })
            .expect("not flat");
        assert_eq!(scale_gap(&scores, 0), 0.0);
        assert!(scale_gap(&scores, 6) > 0.3);
    }
}
