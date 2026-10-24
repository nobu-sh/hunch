//! Scoring rules: tempo accuracy, MIREX key score, calibration and time to lock.

use hunch_core::{Estimate, Key, Mode};
use serde::Serialize;

use crate::fsl10k::TEMPO_TOLERANCE;
use crate::run::SoundRun;

const CALIBRATION_BINS: usize = 10;
const PITCH_CLASSES: u8 = 12;
/// Semitones from a key's tonic up to its fifth.
const FIFTH_SEMITONES: u8 = 7;
/// Semitones from a major tonic up to its relative minor's tonic.
const RELATIVE_MINOR_SEMITONES: u8 = 9;

/// How a tempo estimate relates to the label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TempoError {
    Correct,
    /// Predicted half the labeled tempo.
    Half,
    Double,
    Third,
    Triple,
    Other,
    Missing,
}

impl TempoError {
    pub fn name(self) -> &'static str {
        match self {
            TempoError::Correct => "correct",
            TempoError::Half => "half",
            TempoError::Double => "double",
            TempoError::Third => "third",
            TempoError::Triple => "triple",
            TempoError::Other => "other",
            TempoError::Missing => "missing",
        }
    }

    /// Accuracy2: right up to a factor of 2 or 3.
    pub fn is_octave_correct(self) -> bool {
        !matches!(self, TempoError::Other | TempoError::Missing)
    }
}

pub fn is_tempo_match(predicted: f32, target: f32) -> bool {
    (predicted - target).abs() <= TEMPO_TOLERANCE * target
}

pub fn classify_tempo(predicted: Option<f32>, label: f32) -> TempoError {
    const OCTAVE_ERRORS: [(f32, TempoError); 4] = [
        (0.5, TempoError::Half),
        (2.0, TempoError::Double),
        (1.0 / 3.0, TempoError::Third),
        (3.0, TempoError::Triple),
    ];
    let Some(predicted) = predicted else {
        return TempoError::Missing;
    };
    if is_tempo_match(predicted, label) {
        return TempoError::Correct;
    }
    OCTAVE_ERRORS
        .iter()
        .find(|(factor, _)| is_tempo_match(predicted, label * factor))
        .map_or(TempoError::Other, |(_, error)| *error)
}

#[derive(Debug, Default, Serialize)]
pub struct TempoErrorCounts {
    pub half: usize,
    pub double: usize,
    pub third: usize,
    pub triple: usize,
    pub other: usize,
    pub missing: usize,
}

#[derive(Debug, Serialize)]
pub struct TempoScore {
    pub sounds: usize,
    pub coverage: Option<f64>,
    pub acc1: Option<f64>,
    pub acc2: Option<f64>,
    pub errors: TempoErrorCounts,
}

impl TempoScore {
    pub fn from_errors(errors: impl IntoIterator<Item = TempoError>) -> Self {
        let mut counts = TempoErrorCounts::default();
        let (mut sounds, mut correct, mut octave_correct) = (0, 0, 0);
        for error in errors {
            sounds += 1;
            correct += usize::from(error == TempoError::Correct);
            octave_correct += usize::from(error.is_octave_correct());
            match error {
                TempoError::Correct => {}
                TempoError::Half => counts.half += 1,
                TempoError::Double => counts.double += 1,
                TempoError::Third => counts.third += 1,
                TempoError::Triple => counts.triple += 1,
                TempoError::Other => counts.other += 1,
                TempoError::Missing => counts.missing += 1,
            }
        }
        Self {
            sounds,
            coverage: ratio(sounds - counts.missing, sounds),
            acc1: ratio(correct, sounds),
            acc2: ratio(octave_correct, sounds),
            errors: counts,
        }
    }
}

fn semitones_up(from: Key, to: Key) -> u8 {
    (to.tonic.index() + PITCH_CLASSES - from.tonic.index()) % PITCH_CLASSES
}

fn is_relative(predicted: Key, label: Key) -> bool {
    match (label.mode, predicted.mode) {
        (Mode::Major, Mode::Minor) => semitones_up(label, predicted) == RELATIVE_MINOR_SEMITONES,
        (Mode::Minor, Mode::Major) => semitones_up(predicted, label) == RELATIVE_MINOR_SEMITONES,
        _ => false,
    }
}

/// MIREX weighted key score: exact 1.0, fifth either way 0.5, relative 0.3, parallel 0.2.
pub fn mirex_score(predicted: Key, label: Key) -> f64 {
    let same_mode = predicted.mode == label.mode;
    let interval = semitones_up(label, predicted);
    let is_fifth = interval == FIFTH_SEMITONES || interval == PITCH_CLASSES - FIFTH_SEMITONES;
    if predicted == label {
        1.0
    } else if same_mode && is_fifth {
        0.5
    } else if is_relative(predicted, label) {
        0.3
    } else if !same_mode && interval == 0 {
        0.2
    } else {
        0.0
    }
}

/// Exact or relative key: the same notes, which is all autotune needs.
pub fn is_same_scale(predicted: Key, label: Key) -> bool {
    predicted == label || is_relative(predicted, label)
}

#[derive(Debug, Serialize)]
pub struct KeyScore {
    pub sounds: usize,
    pub coverage: Option<f64>,
    /// Fraction of keyed sounds where no key was reported.
    pub miss_rate: Option<f64>,
    pub mirex: Option<f64>,
    pub same_scale: Option<f64>,
    pub exact: Option<f64>,
}

impl KeyScore {
    pub fn from_predictions(pairs: impl IntoIterator<Item = (Option<Key>, Key)>) -> Self {
        let (mut sounds, mut covered, mut same_scale, mut exact) = (0, 0, 0, 0);
        let mut mirex_total = 0.0;
        for (predicted, label) in pairs {
            sounds += 1;
            let Some(predicted) = predicted else {
                continue;
            };
            covered += 1;
            mirex_total += mirex_score(predicted, label);
            same_scale += usize::from(is_same_scale(predicted, label));
            exact += usize::from(predicted == label);
        }
        Self {
            sounds,
            coverage: ratio(covered, sounds),
            miss_rate: ratio(sounds - covered, sounds),
            mirex: (sounds > 0).then(|| mirex_total / sounds as f64),
            same_scale: ratio(same_scale, sounds),
            exact: ratio(exact, sounds),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct AtonalScore {
    pub sounds: usize,
    /// Fraction of atonal sounds where a key was reported anyway.
    pub false_key_rate: Option<f64>,
}

impl AtonalScore {
    pub fn from_predictions(predictions: impl IntoIterator<Item = Option<Key>>) -> Self {
        let (mut sounds, mut false_keys) = (0, 0);
        for predicted in predictions {
            sounds += 1;
            false_keys += usize::from(predicted.is_some());
        }
        Self {
            sounds,
            false_key_rate: ratio(false_keys, sounds),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct CalibrationBin {
    pub low: f32,
    pub high: f32,
    pub count: usize,
    pub accuracy: Option<f64>,
}

/// Reliability curve: accuracy per equal-width confidence bin.
pub fn calibrate(points: impl IntoIterator<Item = (f32, bool)>) -> Vec<CalibrationBin> {
    let mut counts = [0usize; CALIBRATION_BINS];
    let mut correct = [0usize; CALIBRATION_BINS];
    for (confidence, is_correct) in points {
        let bin = ((confidence.clamp(0.0, 1.0) * CALIBRATION_BINS as f32) as usize)
            .min(CALIBRATION_BINS - 1);
        counts[bin] += 1;
        correct[bin] += usize::from(is_correct);
    }
    (0..CALIBRATION_BINS)
        .map(|bin| CalibrationBin {
            low: bin as f32 / CALIBRATION_BINS as f32,
            high: (bin + 1) as f32 / CALIBRATION_BINS as f32,
            count: counts[bin],
            accuracy: ratio(correct[bin], counts[bin]),
        })
        .collect()
}

/// Seconds of audio after which the estimate became correct and stayed correct,
/// or `None` if the final estimate is wrong.
pub fn lock_seconds(run: &SoundRun, is_correct: impl Fn(&Estimate) -> bool) -> Option<f32> {
    if !is_correct(&run.last) {
        return None;
    }
    let trailing_correct = run
        .timeline
        .iter()
        .rev()
        .take_while(|estimate| is_correct(estimate))
        .last();
    Some(trailing_correct.unwrap_or(&run.last).seconds_analyzed)
}

#[derive(Debug, Serialize)]
pub struct LockStats {
    pub sounds_ending_correct: usize,
    pub median_seconds: Option<f32>,
}

impl LockStats {
    pub fn from_seconds(seconds: impl IntoIterator<Item = f32>) -> Self {
        let mut seconds: Vec<f32> = seconds.into_iter().collect();
        seconds.sort_by(f32::total_cmp);
        Self {
            sounds_ending_correct: seconds.len(),
            median_seconds: median(&seconds),
        }
    }
}

fn median(sorted: &[f32]) -> Option<f32> {
    let middle = sorted.len() / 2;
    match sorted.len() {
        0 => None,
        len if len % 2 == 1 => Some(sorted[middle]),
        _ => Some((sorted[middle - 1] + sorted[middle]) / 2.0),
    }
}

pub fn ratio(numerator: usize, denominator: usize) -> Option<f64> {
    (denominator > 0).then(|| numerator as f64 / denominator as f64)
}

#[cfg(test)]
mod tests {
    use hunch_core::PitchClass;

    use super::*;

    fn key(index: u8, mode: Mode) -> Key {
        let tonic = PitchClass::new(index).expect("valid pitch class");
        Key { tonic, mode }
    }

    fn estimate_at(seconds: f32, bpm: Option<f32>) -> Estimate {
        Estimate {
            key: None,
            key_confidence: 0.0,
            bpm,
            bpm_confidence: 0.0,
            bpm_alternate: None,
            seconds_analyzed: seconds,
        }
    }

    #[test]
    fn classifies_tempo_octave_errors() {
        assert_eq!(classify_tempo(Some(141.0), 140.0), TempoError::Correct);
        assert_eq!(classify_tempo(Some(70.0), 140.0), TempoError::Half);
        assert_eq!(classify_tempo(Some(280.0), 140.0), TempoError::Double);
        assert_eq!(classify_tempo(Some(30.0), 90.0), TempoError::Third);
        assert_eq!(classify_tempo(Some(270.0), 90.0), TempoError::Triple);
        assert_eq!(classify_tempo(Some(120.0), 140.0), TempoError::Other);
        assert_eq!(classify_tempo(None, 140.0), TempoError::Missing);
    }

    #[test]
    fn tempo_tolerance_is_four_percent_of_the_label() {
        assert!(is_tempo_match(104.0, 100.0));
        assert!(!is_tempo_match(104.5, 100.0));
    }

    #[test]
    fn tempo_score_counts_accuracies_and_errors() {
        let score = TempoScore::from_errors([
            TempoError::Correct,
            TempoError::Half,
            TempoError::Other,
            TempoError::Missing,
        ]);
        assert_eq!(score.sounds, 4);
        assert_eq!(score.coverage, Some(0.75));
        assert_eq!(score.acc1, Some(0.25));
        assert_eq!(score.acc2, Some(0.5));
        assert_eq!(score.errors.half, 1);
        assert_eq!(score.errors.missing, 1);
    }

    #[test]
    fn mirex_scores_related_keys() {
        let c_major = key(0, Mode::Major);
        assert_eq!(mirex_score(c_major, c_major), 1.0);
        assert_eq!(mirex_score(key(7, Mode::Major), c_major), 0.5);
        assert_eq!(mirex_score(key(5, Mode::Major), c_major), 0.5);
        assert_eq!(mirex_score(key(9, Mode::Minor), c_major), 0.3);
        assert_eq!(mirex_score(key(0, Mode::Minor), c_major), 0.2);
        assert_eq!(mirex_score(key(2, Mode::Major), c_major), 0.0);
        assert_eq!(mirex_score(key(7, Mode::Minor), c_major), 0.0);
    }

    #[test]
    fn relative_keys_work_from_minor_labels() {
        let a_minor = key(9, Mode::Minor);
        assert_eq!(mirex_score(key(0, Mode::Major), a_minor), 0.3);
        assert!(is_same_scale(key(0, Mode::Major), a_minor));
        assert!(!is_same_scale(key(9, Mode::Major), a_minor));
    }

    #[test]
    fn key_score_counts_missing_keys_as_wrong() {
        let c_major = key(0, Mode::Major);
        let score = KeyScore::from_predictions([
            (Some(c_major), c_major),
            (Some(key(9, Mode::Minor)), c_major),
            (None, c_major),
        ]);
        assert_eq!(score.miss_rate, Some(1.0 / 3.0));
        assert_eq!(score.same_scale, Some(2.0 / 3.0));
        assert_eq!(score.exact, Some(1.0 / 3.0));
        let mirex = score.mirex.expect("mirex");
        assert!((mirex - 1.3 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn calibration_bins_by_confidence() {
        let bins = calibrate([(0.05, true), (0.05, false), (0.95, true), (1.0, true)]);
        assert_eq!(bins.len(), CALIBRATION_BINS);
        assert_eq!(bins[0].count, 2);
        assert_eq!(bins[0].accuracy, Some(0.5));
        assert_eq!(bins[9].count, 2);
        assert_eq!(bins[5].accuracy, None);
    }

    #[test]
    fn lock_time_is_start_of_final_correct_run() {
        let run = SoundRun {
            timeline: vec![
                estimate_at(1.0, Some(140.0)),
                estimate_at(2.0, Some(70.0)),
                estimate_at(3.0, Some(140.0)),
                estimate_at(4.0, Some(140.0)),
            ],
            last: estimate_at(4.2, Some(140.0)),
        };
        let is_correct = |e: &Estimate| e.bpm == Some(140.0);
        assert_eq!(lock_seconds(&run, is_correct), Some(3.0));
    }

    #[test]
    fn lock_time_falls_back_to_final_estimate() {
        let run = SoundRun {
            timeline: vec![estimate_at(1.0, None)],
            last: estimate_at(1.5, Some(140.0)),
        };
        let is_correct = |e: &Estimate| e.bpm == Some(140.0);
        assert_eq!(lock_seconds(&run, is_correct), Some(1.5));

        let wrong_at_end = SoundRun {
            timeline: Vec::new(),
            last: estimate_at(1.5, None),
        };
        assert_eq!(lock_seconds(&wrong_at_end, is_correct), None);
    }

    #[test]
    fn median_handles_even_and_odd_counts() {
        assert_eq!(
            LockStats::from_seconds([3.0, 1.0, 2.0]).median_seconds,
            Some(2.0)
        );
        assert_eq!(
            LockStats::from_seconds([4.0, 1.0]).median_seconds,
            Some(2.5)
        );
        assert_eq!(LockStats::from_seconds([]).median_seconds, None);
    }
}
