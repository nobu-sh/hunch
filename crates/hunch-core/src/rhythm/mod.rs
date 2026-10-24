//! Tempo detection.

#[derive(Debug, Clone, PartialEq)]
pub struct TempoEstimate {
    /// `None` until there's enough rhythmic evidence.
    pub bpm: Option<f32>,
    /// 0..=1, how likely `bpm` is right, including its tempo octave.
    pub confidence: f32,
    /// The most likely other tempo octave (e.g. half or double `bpm`), when the octave call is unsure.
    pub alternate: Option<f32>,
}

/// Names of `OctaveCandidate::features`, in order. Fitted weights are keyed by these.
pub const OCTAVE_FEATURE_NAMES: &[&str] = &[];

/// One tempo-octave candidate and the features the octave decision scores it with.
/// Exposed so `hunch-eval` can dump features and fit the decision weights offline.
#[derive(Debug, Clone, PartialEq)]
pub struct OctaveCandidate {
    pub bpm: f32,
    pub features: Vec<f32>,
}

/// Streaming tempo detector. Accumulates evidence over everything pushed since the last reset.
pub struct TempoDetector {
    sample_rate: u32,
}

impl TempoDetector {
    pub fn new(sample_rate: u32) -> Self {
        Self { sample_rate }
    }

    /// Mono f32 samples in [-1, 1] at the rate given to `new`.
    pub fn push(&mut self, _samples: &[f32]) {
        let _ = self.sample_rate;
    }

    pub fn estimate(&self) -> TempoEstimate {
        TempoEstimate {
            bpm: None,
            confidence: 0.0,
            alternate: None,
        }
    }

    /// The candidates behind the current estimate, related by tempo octave.
    pub fn octave_candidates(&self) -> Vec<OctaveCandidate> {
        Vec::new()
    }

    pub fn reset(&mut self) {}
}
