use std::f32::consts::PI;

use super::*;
use crate::test_audio::{SAMPLE_RATE, beat, sample_count};

const CHUNK_SECONDS: f32 = 0.1;

/// A quiet sustained chord: sound with no onsets, like a breakdown pad.
fn pad(seconds: f32) -> Vec<f32> {
    (0..sample_count(seconds))
        .map(|i| {
            let t = i as f32 / SAMPLE_RATE as f32;
            [220.0, 277.18, 329.63]
                .iter()
                .map(|hz| 0.05 * (2.0 * PI * hz * t).sin())
                .sum()
        })
        .collect()
}

/// Plays `audio` through one window's analyzer and detector, returning the changes it reports.
fn changes_in(audio: &[f32]) -> Vec<TempoChange> {
    let mut analyzer = Analyzer::new(SAMPLE_RATE);
    let mut detector = ChangeDetector::new();
    let mut changes = Vec::new();
    for chunk in audio.chunks(sample_count(CHUNK_SECONDS)) {
        let Some(estimate) = analyzer.push(chunk) else {
            continue;
        };
        let observation = Observation {
            analyzer: &analyzer,
            estimate: &estimate,
        };
        changes.extend(detector.update(observation));
    }
    changes
}

#[test]
fn a_tempo_step_is_split_at_the_step() {
    let step_seconds = 30.0;
    let audio = [beat(120.0, step_seconds), beat(140.0, 30.0)].concat();
    let changes = changes_in(&audio);

    let change = changes.first().expect("a change after the step");
    assert!(
        (change.at_seconds - step_seconds).abs() <= 1.0,
        "change point {change:?}"
    );
    assert!((change.old_bpm - 120.0).abs() <= 3.0, "{change:?}");
    assert!((change.new_bpm - 140.0).abs() <= 3.0, "{change:?}");
}

// The tempo model reads this pattern at 60 BPM as 120, so the half-time case uses 150 -> 75,
// where the recent estimate does read the new octave.
#[test]
fn half_time_is_not_split() {
    let audio = [beat(150.0, 30.0), beat(75.0, 30.0)].concat();
    assert_eq!(changes_in(&audio), Vec::new());
}

#[test]
fn a_recent_estimate_at_double_the_window_tempo_is_not_split() {
    // The window reads 75 BPM; the last 6 s alone read 150.
    assert_eq!(changes_in(&beat(75.0, 60.0)), Vec::new());
}

#[test]
fn a_breakdown_without_onsets_is_not_split() {
    let audio = [beat(120.0, 30.0), pad(8.0), beat(120.0, 30.0)].concat();
    assert_eq!(changes_in(&audio), Vec::new());
}

#[test]
fn a_steady_beat_is_not_split() {
    assert_eq!(changes_in(&beat(128.0, 60.0)), Vec::new());
}

#[test]
fn reset_forgets_the_window() {
    let mut analyzer = Analyzer::new(SAMPLE_RATE);
    let mut detector = ChangeDetector::new();
    let feed = |audio: &[f32], analyzer: &mut Analyzer, detector: &mut ChangeDetector| {
        let mut changes = Vec::new();
        for chunk in audio.chunks(sample_count(CHUNK_SECONDS)) {
            if let Some(estimate) = analyzer.push(chunk) {
                changes.extend(detector.update(Observation {
                    analyzer,
                    estimate: &estimate,
                }));
            }
        }
        changes
    };
    feed(&beat(120.0, 30.0), &mut analyzer, &mut detector);
    analyzer.reset();
    detector.reset();

    // A new window that starts on the new beat has no earlier beat to differ from.
    let changes = feed(&beat(140.0, 20.0), &mut analyzer, &mut detector);
    assert_eq!(changes, Vec::new());
}
