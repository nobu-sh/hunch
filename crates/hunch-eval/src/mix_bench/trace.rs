//! One pass of the analyzer over a clip, recording everything the change detectors read at each
//! estimate (every 0.5 s), so detector settings can be swept without re-analyzing audio.

use hunch_core::rhythm::TempoEstimate;
use hunch_core::tonal::{ChromaBlock, KeyDetectorConfig};
use hunch_core::{Analyzer, Estimate};

use super::features::{FrontEnd, TIMBRE_BANDS};
use crate::resample::ANALYSIS_SAMPLE_RATE;
use crate::run;

/// The analyzer's estimate cadence; every trace step is one estimate.
pub const TICK_SECONDS: f32 = 0.5;
/// Lengths of recent onset history the tempo is re-estimated on at every tick.
pub const RECENT_TEMPO_SECONDS: [f32; 4] = [4.0, 6.0, 8.0, 12.0];

#[derive(Debug, Clone, Copy)]
pub struct Tempo {
    pub bpm: Option<f32>,
    pub confidence: f32,
}

impl From<TempoEstimate> for Tempo {
    fn from(estimate: TempoEstimate) -> Self {
        Self {
            bpm: estimate.bpm,
            confidence: estimate.confidence,
        }
    }
}

pub struct Tick {
    /// Key evidence from the 0.5 s before this tick.
    pub block: ChromaBlock,
    /// Indexed like `RECENT_TEMPO_SECONDS`.
    pub recent_tempo: [Tempo; RECENT_TEMPO_SECONDS.len()],
    pub timbre: [f32; TIMBRE_BANDS],
}

pub struct Trace {
    pub ticks: Vec<Tick>,
    /// All-band onset envelope at `features::frame_rate()`.
    pub flux: Vec<f32>,
}

impl Trace {
    pub fn seconds_of(tick: usize) -> f32 {
        (tick + 1) as f32 * TICK_SECONDS
    }
}

pub fn analyze(samples: &[f32]) -> Trace {
    let config = KeyDetectorConfig {
        track_blocks: true,
        ..KeyDetectorConfig::default()
    };
    let mut analyzer = Analyzer::with_key_config(ANALYSIS_SAMPLE_RATE, config);
    let mut front_end = FrontEnd::new();
    let mut ticks = Vec::new();
    for chunk in samples.chunks(run::chunk_len()) {
        front_end.push(chunk);
        if analyzer.push(chunk).is_none() {
            continue;
        }
        let block = analyzer
            .key_detector_mut()
            .take_block()
            .expect("blocks are tracked");
        let tempo = analyzer.tempo_detector();
        ticks.push(Tick {
            block,
            recent_tempo: RECENT_TEMPO_SECONDS.map(|seconds| tempo.estimate_recent(seconds).into()),
            timbre: front_end.take_timbre(),
        });
    }
    Trace {
        ticks,
        flux: front_end.flux,
    }
}

/// Plays `samples` through an analyzer that starts at `from_seconds` and returns its estimate
/// at each of `at_seconds` (absolute times, after `from_seconds`), `None` before its first one.
pub fn estimates_at(
    samples: &[f32],
    key_config: KeyDetectorConfig,
    from_seconds: f32,
    at_seconds: &[f32],
) -> Vec<Option<Estimate>> {
    let rate = ANALYSIS_SAMPLE_RATE as f32;
    let start = ((from_seconds * rate) as usize).min(samples.len());
    let mut analyzer = Analyzer::with_key_config(ANALYSIS_SAMPLE_RATE, key_config);
    let mut latest: Option<Estimate> = None;
    let mut results = vec![None; at_seconds.len()];
    let mut is_filled = vec![false; at_seconds.len()];
    let mut position = start;
    for chunk in samples[start..].chunks(run::chunk_len()) {
        position += chunk.len();
        if let Some(estimate) = analyzer.push(chunk) {
            latest = Some(estimate);
        }
        let now = position as f32 / rate;
        for ((result, filled), &at) in results.iter_mut().zip(&mut is_filled).zip(at_seconds) {
            if !*filled && now >= at - 1e-3 {
                *result = latest.clone();
                *filled = true;
            }
        }
    }
    results
}
