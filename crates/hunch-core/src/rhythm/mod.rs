//! Tempo detection.
//!
//! Onset envelopes in kick, snare and hat bands are autocorrelated to find the dominant beat
//! period. Its tempo-octave relatives (×½, ×2, ×⅔, …) become candidates, and a linear model over
//! beat-structure features picks the one producers would label the beat with. The model's weights
//! live in `octave_weights.rs` and are fitted offline by `hunch-eval fit-octave`.

mod features;
mod octave_weights;
mod onset;
mod periodicity;

use self::features::{BeatEvidence, FEATURE_COUNT, FEATURE_NAMES};
use self::onset::{Band, OnsetEnvelopes};
use self::periodicity::{LagRange, bpm_to_lag, dominant_period, lag_to_bpm};

/// Audio needed before the first estimate.
const MIN_SECONDS: f32 = 4.0;
/// Range the dominant period is searched in. Every tempo in the candidate range has an octave
/// relative in here, so the right family is always found.
const SEARCH_MIN_BPM: f32 = 60.0;
const SEARCH_MAX_BPM: f32 = 180.0;
/// Range candidates must fall in.
const CANDIDATE_MIN_BPM: f32 = 50.0;
const CANDIDATE_MAX_BPM: f32 = 250.0;
/// Tempo ratios to the dominant period that become candidates.
const OCTAVE_RATIOS: [f32; 7] = [1.0 / 3.0, 0.5, 2.0 / 3.0, 1.0, 1.5, 2.0, 3.0];
/// The longest lag any feature reads, in beats of the slowest candidate.
const MAX_LAG_BEATS: f32 = 4.0;
/// Comb strength at or below this is treated as no rhythm; at `FULL_PERIODICITY` and above the
/// periodicity factor of the confidence is 1.
const MIN_PERIODICITY: f32 = 0.15;
const FULL_PERIODICITY: f32 = 0.5;
/// The runner-up is offered as `alternate` when its probability is within this of the winner's.
const ALTERNATE_MARGIN: f32 = 0.3;

#[derive(Debug, Clone, PartialEq)]
pub struct TempoEstimate {
    /// `None` until there's enough rhythmic evidence.
    pub bpm: Option<f32>,
    /// 0..=1, how likely `bpm` is right, including its tempo octave.
    pub confidence: f32,
    /// The most likely other tempo octave (e.g. half or double `bpm`), when the octave call is unsure.
    pub alternate: Option<f32>,
}

impl TempoEstimate {
    const NONE: Self = Self {
        bpm: None,
        confidence: 0.0,
        alternate: None,
    };
}

/// Names of `OctaveCandidate::features`, in order. Fitted weights are keyed by these.
///
/// A "beat" is one period of the candidate tempo. ACF values are the normalized, mean-removed
/// autocorrelation of a band's onset envelope (1 = perfectly periodic, ~0 = unrelated), taking
/// the peak within ±2% of the lag.
/// - `log2_tempo`, `log2_tempo_squared`: `log2(bpm / 120)` and its square, the tempo prior.
/// - `combined_beat`, `combined_half_beat`, `combined_double_beat`: all-band ACF at 1, ½ and 2 beats.
/// - `low_beat`, `low_two_beats`, `low_four_beats`: kick band (30–150 Hz) ACF at 1, 2 and 4 beats.
/// - `mid_beat`, `mid_two_beats`, `mid_four_beats`: snare band (150 Hz–2 kHz) ACF at 1, 2 and 4
///   beats. A backbeat is strong at 2 and weak at 1.
/// - `high_beat`, `high_half_beat`, `high_quarter_beat`: hat band (5–11 kHz) ACF at 1, ½ and ¼
///   beat, i.e. hat subdivision.
/// - `onset_density_deviation`, `hat_density_deviation`: `(log2(onsets per beat) − log2 3)²` for
///   all onsets and for hat-band onsets: how far the beat is from 8th/16th-note density.
/// - `grid_salience`: mean combined envelope on the best-aligned pulse grid over its overall mean.
/// - `grid_offbeat_contrast`: `(on − off) / (on + off)` for that grid vs the half-beat-shifted one.
pub const OCTAVE_FEATURE_NAMES: &[&str] = &FEATURE_NAMES;

const _: () = assert!(octave_weights::WEIGHTS.len() == FEATURE_COUNT);

/// One tempo-octave candidate and the features the octave decision scores it with.
/// Exposed so `hunch-eval` can dump features and fit the decision weights offline.
#[derive(Debug, Clone, PartialEq)]
pub struct OctaveCandidate {
    pub bpm: f32,
    pub features: Vec<f32>,
}

/// Streaming tempo detector. Accumulates evidence over the last ~30 s pushed since the last reset.
pub struct TempoDetector {
    onsets: OnsetEnvelopes,
}

/// The candidates for the current audio and how periodic it is.
struct Analysis {
    candidates: Vec<OctaveCandidate>,
    /// 0..=1 factor from the dominant period's comb strength; 0 means no rhythm.
    periodicity: f32,
}

impl TempoDetector {
    /// Works at any sample rate; the extension uses 22,050 Hz.
    pub fn new(sample_rate: u32) -> Self {
        Self {
            onsets: OnsetEnvelopes::new(sample_rate),
        }
    }

    /// Mono f32 samples in [-1, 1] at the rate given to `new`.
    pub fn push(&mut self, samples: &[f32]) {
        self.onsets.push(samples);
    }

    pub fn estimate(&self) -> TempoEstimate {
        let Some(analysis) = self.analyze() else {
            return TempoEstimate::NONE;
        };
        if analysis.periodicity <= 0.0 {
            return TempoEstimate::NONE;
        }
        decide(&analysis)
    }

    /// The candidates behind the current estimate, related by tempo octave.
    /// Empty until there's enough audio, or when it's silent.
    pub fn octave_candidates(&self) -> Vec<OctaveCandidate> {
        self.analyze()
            .map(|analysis| analysis.candidates)
            .unwrap_or_default()
    }

    pub fn reset(&mut self) {
        self.onsets.reset();
    }

    fn analyze(&self) -> Option<Analysis> {
        let frame_rate = self.onsets.frame_rate();
        let history = self.onsets.history();
        if (history.len() as f32) < MIN_SECONDS * frame_rate {
            return None;
        }

        let envelopes =
            Band::ALL.map(|band| history.iter().map(|frame| frame[band.index()]).collect());
        let max_lag =
            (MAX_LAG_BEATS * bpm_to_lag(CANDIDATE_MIN_BPM, frame_rate)).ceil() as usize + 1;
        let evidence = BeatEvidence::new(envelopes, frame_rate, max_lag)?;

        let search = LagRange {
            shortest: bpm_to_lag(SEARCH_MAX_BPM, frame_rate).floor() as usize,
            longest: bpm_to_lag(SEARCH_MIN_BPM, frame_rate).ceil() as usize,
        };
        let dominant = dominant_period(evidence.combined(), search, frame_rate)?;
        let dominant_bpm = lag_to_bpm(dominant.lag, frame_rate);
        let candidates = candidate_tempi(dominant_bpm)
            .map(|bpm| OctaveCandidate {
                bpm,
                features: evidence.candidate_features(bpm).to_vec(),
            })
            .collect();

        Some(Analysis {
            candidates,
            periodicity: periodicity_factor(dominant.strength),
        })
    }
}

/// The octave relatives of `dominant_bpm` inside the candidate range, slowest first.
fn candidate_tempi(dominant_bpm: f32) -> impl Iterator<Item = f32> {
    OCTAVE_RATIOS
        .into_iter()
        .map(move |ratio| dominant_bpm * ratio)
        .filter(|bpm| (CANDIDATE_MIN_BPM..=CANDIDATE_MAX_BPM).contains(bpm))
}

fn periodicity_factor(strength: f32) -> f32 {
    ((strength - MIN_PERIODICITY) / (FULL_PERIODICITY - MIN_PERIODICITY)).clamp(0.0, 1.0)
}

/// Picks the highest-scoring candidate; offers the runner-up as alternate when it's close.
fn decide(analysis: &Analysis) -> TempoEstimate {
    let scores: Vec<f32> = analysis
        .candidates
        .iter()
        .map(|candidate| octave_score(&candidate.features))
        .collect();
    let probabilities = softmax(&scores);

    let mut ranked: Vec<usize> = (0..probabilities.len()).collect();
    ranked.sort_by(|&a, &b| probabilities[b].total_cmp(&probabilities[a]));
    let Some(&winner) = ranked.first() else {
        return TempoEstimate::NONE;
    };
    let alternate = ranked
        .get(1)
        .filter(|&&runner_up| probabilities[winner] - probabilities[runner_up] <= ALTERNATE_MARGIN)
        .map(|&runner_up| analysis.candidates[runner_up].bpm);

    TempoEstimate {
        bpm: Some(analysis.candidates[winner].bpm),
        confidence: probabilities[winner] * analysis.periodicity,
        alternate,
    }
}

/// `BIAS + Σ wᵢ·(fᵢ − meanᵢ)/stdᵢ` with the weights in `octave_weights.rs`.
fn octave_score(features: &[f32]) -> f32 {
    let weighted: f32 = features
        .iter()
        .zip(octave_weights::FEATURE_MEANS)
        .zip(octave_weights::FEATURE_STDS)
        .zip(octave_weights::WEIGHTS)
        .map(|(((value, mean), std), weight)| weight * (value - mean) / std)
        .sum();
    octave_weights::BIAS + weighted
}

fn softmax(scores: &[f32]) -> Vec<f32> {
    let max = scores.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let exponentials: Vec<f32> = scores.iter().map(|score| (score - max).exp()).collect();
    let total: f32 = exponentials.iter().sum();
    exponentials.iter().map(|value| value / total).collect()
}

#[cfg(test)]
mod tests;
