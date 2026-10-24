//! Key detection.

use crate::Key;

#[derive(Debug, Clone, PartialEq)]
pub struct KeyEstimate {
    /// `None` when the audio has no clear tonal center (drums, noise, speech).
    pub key: Option<Key>,
    /// 0..=1, how likely `key` is the right scale. 0 when `key` is `None`.
    pub confidence: f32,
    /// 0..=1, how pitched/harmonic the audio is. Used to decide whether to report a key at all.
    pub tonalness: f32,
}

/// Streaming key detector. Accumulates evidence over everything pushed since the last reset.
pub struct KeyDetector {
    sample_rate: u32,
}

impl KeyDetector {
    pub fn new(sample_rate: u32) -> Self {
        Self { sample_rate }
    }

    /// Mono f32 samples in [-1, 1] at the rate given to `new`.
    pub fn push(&mut self, _samples: &[f32]) {
        let _ = self.sample_rate;
    }

    pub fn estimate(&self) -> KeyEstimate {
        KeyEstimate {
            key: None,
            confidence: 0.0,
            tonalness: 0.0,
        }
    }

    pub fn reset(&mut self) {}
}
