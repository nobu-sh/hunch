//! Beat-structure features that score each tempo-octave candidate.

use super::onset::Band;
use super::periodicity::{Autocorrelation, bpm_to_lag};
use super::repetition::Repetition;

/// Tempo the log-tempo features are measured from.
const REFERENCE_BPM: f32 = 120.0;
/// Onsets per beat the density features measure distance from: 8th notes.
const TYPICAL_ONSETS_PER_BEAT: f32 = 2.0;
/// Clamp on `log2(onsets per beat)` so a band with no onsets stays finite.
const MAX_LOG_DENSITY: f32 = 4.0;
/// An onset is a local maximum this many standard deviations above the envelope mean...
const ONSET_THRESHOLD_STDS: f32 = 0.5;
/// ...and the largest value within this many frames either side (~50 ms at 100 fps).
const ONSET_NEIGHBORHOOD_FRAMES: usize = 5;
const EPSILON: f32 = 1e-6;

/// Names of the features `candidate_features` returns, in order.
pub const FEATURE_NAMES: [&str; FEATURE_COUNT] = [
    "log2_tempo",
    "log2_tempo_squared",
    "combined_beat",
    "combined_half_beat",
    "combined_double_beat",
    "low_beat",
    "low_two_beats",
    "low_four_beats",
    "mid_beat",
    "mid_two_beats",
    "mid_four_beats",
    "high_beat",
    "high_half_beat",
    "high_quarter_beat",
    "onset_density_deviation",
    "hat_density_deviation",
    "grid_salience",
    "grid_offbeat_contrast",
    "loop_beats_off_integer",
    "loop_beats_off_power_of_two",
];
pub const FEATURE_COUNT: usize = 20;

/// Everything measured once per estimate that the per-candidate features read from.
pub struct BeatEvidence {
    combined: Autocorrelation,
    /// Per-band autocorrelations, indexed by `Band::index`. `None` for a band that's silent throughout.
    bands: [Option<Autocorrelation>; Band::ALL.len()],
    /// Raw (not mean-removed) combined envelope, for beat-grid alignment.
    combined_envelope: Vec<f32>,
    frame_rate: f32,
    onsets_per_second: f32,
    hat_onsets_per_second: f32,
    repetition: Option<Repetition>,
}

pub struct EvidenceInput {
    /// Indexed by `Band::index`.
    pub envelopes: [Vec<f32>; Band::ALL.len()],
    pub frame_rate: f32,
    /// The longest lag any feature reads, in frames.
    pub max_lag: usize,
    /// How often the whole pattern repeats, if it does.
    pub repetition: Option<Repetition>,
}

impl BeatEvidence {
    /// `None` when the combined envelope is flat (silence).
    pub fn new(input: EvidenceInput) -> Option<Self> {
        let EvidenceInput {
            envelopes,
            frame_rate,
            max_lag,
            repetition,
        } = input;
        let mut bands =
            Band::ALL.map(|band| Autocorrelation::new(&envelopes[band.index()], max_lag));
        let combined = bands[Band::Combined.index()].take()?;

        let [_, _, high, combined_envelope] = envelopes;
        let seconds = combined_envelope.len() as f32 / frame_rate;
        let onsets_per_second = count_onsets(&combined_envelope) as f32 / seconds;
        let hat_onsets_per_second = count_onsets(&high) as f32 / seconds;
        Some(Self {
            combined,
            bands,
            combined_envelope,
            frame_rate,
            onsets_per_second,
            hat_onsets_per_second,
            repetition,
        })
    }

    pub fn combined(&self) -> &Autocorrelation {
        &self.combined
    }

    /// Correlation of `band` near `beat_lag` frames; 0 for a silent band.
    fn strength(&self, band: Band, beat_lag: f32) -> f32 {
        let autocorrelation = match band {
            Band::Combined => Some(&self.combined),
            _ => self.bands[band.index()].as_ref(),
        };
        autocorrelation.map_or(0.0, |acf| acf.peak_near(beat_lag))
    }

    /// Features for a candidate at `bpm`, in `FEATURE_NAMES` order. Always finite.
    pub fn candidate_features(&self, bpm: f32) -> [f32; FEATURE_COUNT] {
        let beat = bpm_to_lag(bpm, self.frame_rate);
        let strength = |band: Band, beats: f32| self.strength(band, beat * beats);
        let log2_tempo = (bpm / REFERENCE_BPM).log2();
        let beats_per_second = bpm / 60.0;
        let grid = grid_alignment(&self.combined_envelope, beat);
        let loop_fit = self
            .repetition
            .map_or(LoopFit::NEUTRAL, |repetition| loop_fit(repetition, beat));

        let features = [
            log2_tempo,
            log2_tempo * log2_tempo,
            strength(Band::Combined, 1.0),
            strength(Band::Combined, 0.5),
            strength(Band::Combined, 2.0),
            strength(Band::Low, 1.0),
            strength(Band::Low, 2.0),
            strength(Band::Low, 4.0),
            strength(Band::Mid, 1.0),
            strength(Band::Mid, 2.0),
            strength(Band::Mid, 4.0),
            strength(Band::High, 1.0),
            strength(Band::High, 0.5),
            strength(Band::High, 0.25),
            density_deviation(self.onsets_per_second / beats_per_second),
            density_deviation(self.hat_onsets_per_second / beats_per_second),
            grid.salience,
            grid.offbeat_contrast,
            loop_fit.off_integer,
            loop_fit.off_power_of_two,
        ];
        features.map(|value| if value.is_finite() { value } else { 0.0 })
    }
}

/// How badly a candidate's beat divides the repetition period, scaled by `Repetition::weight`.
struct LoopFit {
    /// Distance of the beats per repetition from a whole number, 0..=0.5.
    off_integer: f32,
    /// Distance of `log2(beats per repetition)` from a whole number, 0..=0.5.
    off_power_of_two: f32,
}

impl LoopFit {
    /// Without a repetition every candidate fits equally, so the features cancel in the softmax.
    const NEUTRAL: Self = Self {
        off_integer: 0.0,
        off_power_of_two: 0.0,
    };
}

fn loop_fit(repetition: Repetition, beat: f32) -> LoopFit {
    let beats = repetition.lag / beat.max(EPSILON);
    let off_whole = |value: f32| repetition.weight() * (value - value.round()).abs();
    LoopFit {
        off_integer: off_whole(beats),
        off_power_of_two: off_whole(beats.max(EPSILON).log2()),
    }
}

/// Squared octave distance of an onsets-per-beat rate from `TYPICAL_ONSETS_PER_BEAT`.
fn density_deviation(onsets_per_beat: f32) -> f32 {
    let log_density = onsets_per_beat
        .max(EPSILON)
        .log2()
        .clamp(-MAX_LOG_DENSITY, MAX_LOG_DENSITY);
    let deviation = log_density - TYPICAL_ONSETS_PER_BEAT.log2();
    deviation * deviation
}

/// Peaks above `mean + ONSET_THRESHOLD_STDS·std` that dominate their neighborhood.
fn count_onsets(envelope: &[f32]) -> usize {
    let count = envelope.len().max(1) as f32;
    let mean = envelope.iter().sum::<f32>() / count;
    let variance = envelope
        .iter()
        .map(|v| (v - mean) * (v - mean))
        .sum::<f32>()
        / count;
    let threshold = mean + ONSET_THRESHOLD_STDS * variance.sqrt();

    (0..envelope.len())
        .filter(|&frame| {
            let value = envelope[frame];
            if value <= threshold {
                return false;
            }
            let start = frame.saturating_sub(ONSET_NEIGHBORHOOD_FRAMES);
            let end = (frame + ONSET_NEIGHBORHOOD_FRAMES + 1).min(envelope.len());
            // Ties go to the earliest frame so a flat peak counts once.
            let is_earliest_max = envelope[start..frame].iter().all(|&other| other < value);
            let is_max_after = envelope[frame + 1..end].iter().all(|&other| other <= value);
            is_earliest_max && is_max_after
        })
        .count()
}

struct GridAlignment {
    /// Mean envelope on the best-aligned pulse grid, over the overall mean.
    salience: f32,
    /// `(on − off) / (on + off)` for the best grid vs the grid shifted half a beat.
    offbeat_contrast: f32,
}

/// How well a pulse train with period `beat` frames lines up with the envelope's strong onsets.
fn grid_alignment(envelope: &[f32], beat: f32) -> GridAlignment {
    let none = GridAlignment {
        salience: 0.0,
        offbeat_contrast: 0.0,
    };
    let overall_mean = envelope.iter().sum::<f32>() / envelope.len().max(1) as f32;
    let has_full_beat = beat >= 1.0 && (beat as usize) < envelope.len();
    if overall_mean <= EPSILON || !has_full_beat {
        return none;
    }

    let phase_count = beat.floor() as usize;
    let Some((best_phase, on_beat)) = (0..phase_count)
        .map(|phase| (phase, grid_mean(envelope, phase as f32, beat)))
        .max_by(|a, b| a.1.total_cmp(&b.1))
    else {
        return none;
    };
    let off_beat = grid_mean(envelope, best_phase as f32 + beat / 2.0, beat);
    GridAlignment {
        salience: on_beat / overall_mean,
        offbeat_contrast: (on_beat - off_beat) / (on_beat + off_beat + EPSILON),
    }
}

/// Mean envelope value at frames `phase + k·period`, rounded to the nearest frame.
fn grid_mean(envelope: &[f32], phase: f32, period: f32) -> f32 {
    let mut sum = 0.0;
    let mut count = 0;
    let mut position = phase;
    while let Some(&value) = envelope.get(position.round() as usize) {
        sum += value;
        count += 1;
        position += period;
    }
    if count == 0 {
        return 0.0;
    }
    sum / count as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_separated_peaks_once() {
        let mut envelope = vec![0.0; 100];
        for start in [10, 40, 70] {
            envelope[start] = 1.0;
            envelope[start + 1] = 1.0;
            envelope[start + 2] = 0.5;
        }
        assert_eq!(count_onsets(&envelope), 3);
    }

    #[test]
    fn density_deviation_is_zero_at_typical_rate() {
        assert!(density_deviation(TYPICAL_ONSETS_PER_BEAT).abs() < 1e-6);
        assert!(density_deviation(0.0).is_finite());
    }

    #[test]
    fn grid_aligned_with_pulses_is_salient() {
        let envelope: Vec<f32> = (0..400)
            .map(|frame| if frame % 40 == 7 { 1.0 } else { 0.0 })
            .collect();
        let aligned = grid_alignment(&envelope, 40.0);
        assert!(aligned.salience > 30.0);
        assert!(aligned.offbeat_contrast > 0.9);

        let silent = grid_alignment(&[0.0; 400], 40.0);
        assert_eq!(silent.salience, 0.0);
    }
}
