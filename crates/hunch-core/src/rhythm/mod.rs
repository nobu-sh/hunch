//! Tempo detection.
//!
//! Onset envelopes in kick, snare and hat bands are autocorrelated to find the strongest beat
//! periods. Their tempo-octave relatives (×½, ×2, ×⅔, …) become candidates, and a linear model over
//! beat-structure features picks the one producers would label the beat with. The model's weights
//! live in `octave_weights.rs` and are fitted offline by `hunch-eval fit-octave`.

mod features;
mod octave_weights;
mod onset;
mod periodicity;
mod repetition;

use self::features::{BeatEvidence, EvidenceInput, FEATURE_COUNT, FEATURE_NAMES};
use self::onset::{Band, OnsetEnvelopes};
use self::periodicity::{LagRange, Period, bpm_to_lag, lag_to_bpm, strongest_periods};
use self::repetition::{Repetition, RepetitionFinder};

/// Audio needed before the first estimate.
const MIN_SECONDS: f32 = 4.0;
/// Range the seed periods are searched in. Every tempo in the candidate range has an octave
/// relative in here, so the right family is found as long as its period is a peak.
const SEARCH_MIN_BPM: f32 = 60.0;
const SEARCH_MAX_BPM: f32 = 180.0;
/// Range candidates must fall in.
const CANDIDATE_MIN_BPM: f32 = 50.0;
const CANDIDATE_MAX_BPM: f32 = 250.0;
/// Tempo ratios to each seed period that become candidates.
const OCTAVE_RATIOS: [f32; 7] = [1.0 / 3.0, 0.5, 2.0 / 3.0, 1.0, 1.5, 2.0, 3.0];
/// Seed periods: the strongest, plus the next strongest outside its family when that one is at
/// least this fraction as strong. Catches beats labeled at e.g. 3:4 to the strongest pulse.
const MAX_SEEDS: usize = 2;
const MIN_SEED_STRENGTH_RATIO: f32 = 0.6;
/// Candidates closer than this fraction of their tempo are the same candidate.
const SAME_TEMPO_TOLERANCE: f32 = 0.03;
/// The longest lag any feature reads, in beats of the slowest candidate.
const MAX_LAG_BEATS: f32 = 4.0;
/// The shortest repetition searched for, in beats of the fastest candidate: one bar.
const MIN_REPETITION_BEATS: f32 = 4.0;
/// Comb strength at or below the no-rhythm threshold gives `bpm: None`; at `FULL_PERIODICITY`
/// and above, the periodicity factor of the confidence is 1. The threshold is `NOISE_STDS` noise
/// standard deviations (noise autocorrelates with std ~1/sqrt(frames)), and at least `MIN_PERIODICITY`.
const NOISE_STDS: f32 = 3.0;
const MIN_PERIODICITY: f32 = 0.06;
const FULL_PERIODICITY: f32 = 0.4;
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
/// the peak within ±2% (at least ±1 frame) of the lag.
/// - `log2_tempo`, `log2_tempo_squared`: `log2(bpm / 120)` and its square, the tempo prior.
/// - `combined_beat`, `combined_half_beat`, `combined_double_beat`: all-band ACF at 1, ½ and 2 beats.
/// - `low_beat`, `low_two_beats`, `low_four_beats`: kick band (30–150 Hz) ACF at 1, 2 and 4 beats.
/// - `mid_beat`, `mid_two_beats`, `mid_four_beats`: snare band (150 Hz–2 kHz) ACF at 1, 2 and 4
///   beats. A backbeat is strong at 2 and weak at 1.
/// - `high_beat`, `high_half_beat`, `high_quarter_beat`: hat band (5–11 kHz) ACF at 1, ½ and ¼
///   beat, i.e. hat subdivision.
/// - `onset_density_deviation`, `hat_density_deviation`: `(log2(onsets per beat) − 1)²` for all
///   onsets and for hat-band onsets: how many octaves the density is from 8th notes.
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
    repetition: RepetitionFinder,
}

/// The candidates for the current audio and how periodic it is.
struct Analysis {
    candidates: Vec<OctaveCandidate>,
    /// 0..=1 factor from the strongest period's comb strength; 0 means no rhythm.
    periodicity: f32,
}

impl TempoDetector {
    /// Works at any sample rate; the extension uses 22,050 Hz.
    pub fn new(sample_rate: u32) -> Self {
        let onsets = OnsetEnvelopes::new(sample_rate);
        Self {
            repetition: RepetitionFinder::new(onsets.history_capacity()),
            onsets,
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

        let envelopes: [Vec<f32>; Band::ALL.len()] =
            Band::ALL.map(|band| history.iter().map(|frame| frame[band.index()]).collect());
        let max_lag =
            (MAX_LAG_BEATS * bpm_to_lag(CANDIDATE_MIN_BPM, frame_rate)).ceil() as usize + 1;
        let shortest_repetition =
            (MIN_REPETITION_BEATS * bpm_to_lag(CANDIDATE_MAX_BPM, frame_rate)).floor() as usize;
        let repetition = self
            .repetition
            .find(&envelopes[Band::Combined.index()], shortest_repetition);
        let evidence = BeatEvidence::new(EvidenceInput {
            envelopes,
            frame_rate,
            max_lag,
            repetition,
        })?;

        let search = LagRange {
            shortest: bpm_to_lag(SEARCH_MAX_BPM, frame_rate).floor() as usize,
            longest: bpm_to_lag(SEARCH_MIN_BPM, frame_rate).ceil() as usize,
        };
        let periods = strongest_periods(evidence.combined(), search, frame_rate);
        let strongest = *periods.first()?;
        let candidates = candidate_tempi(CandidateSources {
            periods: &periods,
            repetition,
            frame_rate,
        })
        .into_iter()
        .map(|bpm| OctaveCandidate {
            bpm,
            features: evidence.candidate_features(bpm).to_vec(),
        })
        .collect();

        Some(Analysis {
            candidates,
            periodicity: periodicity_factor(PeriodicityInput {
                strength: strongest.strength,
                frame_count: history.len(),
            }),
        })
    }
}

struct CandidateSources<'a> {
    /// Strongest first and non-empty.
    periods: &'a [Period],
    repetition: Option<Repetition>,
    frame_rate: f32,
}

/// The tempi that fit a strong repetition, then the octave relatives of the seed periods, inside
/// the candidate range, slowest first.
fn candidate_tempi(sources: CandidateSources) -> Vec<f32> {
    let CandidateSources {
        periods,
        repetition,
        frame_rate,
    } = sources;
    let mut seed_tempi: Vec<f32> = Vec::new();
    let mut seeds = 0;
    for period in periods {
        let is_strong_enough = period.strength >= MIN_SEED_STRENGTH_RATIO * periods[0].strength;
        if seeds == MAX_SEEDS || !is_strong_enough {
            break;
        }
        let bpm = lag_to_bpm(period.lag, frame_rate);
        if contains_tempo(&seed_tempi, bpm) {
            continue;
        }
        seeds += 1;
        seed_tempi.extend(OCTAVE_RATIOS.map(|ratio| bpm * ratio));
    }

    // Loop tempi go first so they win over a slightly-off seed relative of the same tempo.
    let mut tempi =
        repetition.map_or_else(Vec::new, |repetition| loop_tempi(repetition, frame_rate));
    for related in seed_tempi {
        let is_in_range = (CANDIDATE_MIN_BPM..=CANDIDATE_MAX_BPM).contains(&related);
        if is_in_range && !contains_tempo(&tempi, related) {
            tempi.push(related);
        }
    }
    tempi.sort_by(f32::total_cmp);
    tempi
}

/// Tempi with a power-of-two number of beats per repetition, when it's trusted as a loop length.
fn loop_tempi(repetition: Repetition, frame_rate: f32) -> Vec<f32> {
    if repetition.weight() <= 0.0 {
        return Vec::new();
    }
    let one_beat_bpm = lag_to_bpm(repetition.lag, frame_rate);
    (0..u32::BITS)
        .map(|power| one_beat_bpm * 2f32.powi(power as i32))
        .take_while(|&bpm| bpm <= CANDIDATE_MAX_BPM)
        .filter(|&bpm| bpm >= CANDIDATE_MIN_BPM)
        .collect()
}

fn contains_tempo(tempi: &[f32], bpm: f32) -> bool {
    tempi
        .iter()
        .any(|&existing| (existing - bpm).abs() <= SAME_TEMPO_TOLERANCE * bpm)
}

struct PeriodicityInput {
    /// Comb strength of the strongest period.
    strength: f32,
    frame_count: usize,
}

fn periodicity_factor(input: PeriodicityInput) -> f32 {
    let noise_floor = NOISE_STDS / (input.frame_count.max(1) as f32).sqrt();
    let threshold = noise_floor.max(MIN_PERIODICITY);
    if threshold >= FULL_PERIODICITY {
        return 0.0;
    }
    ((input.strength - threshold) / (FULL_PERIODICITY - threshold)).clamp(0.0, 1.0)
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
