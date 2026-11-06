use std::f32::consts::PI;

use super::*;

const SAMPLE_RATE: u32 = 22_050;
const CHUNK_SECONDS: f32 = 0.1;

fn sample_count(seconds: f32) -> usize {
    (seconds * SAMPLE_RATE as f32) as usize
}

/// Deterministic white noise in [-1, 1].
struct Noise(u32);

impl Noise {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.0 >> 8) as f32 / (1u32 << 23) as f32 - 1.0
    }
}

fn kick() -> Vec<f32> {
    let mut phase = 0.0;
    (0..sample_count(0.3))
        .map(|i| {
            let t = i as f32 / SAMPLE_RATE as f32;
            phase += 2.0 * PI * (50.0 + 70.0 * (-t / 0.03).exp()) / SAMPLE_RATE as f32;
            0.9 * phase.sin() * (-t / 0.12).exp()
        })
        .collect()
}

fn hat() -> Vec<f32> {
    let mut noise = Noise(7);
    let mut previous = 0.0;
    (0..sample_count(0.05))
        .map(|i| {
            let t = i as f32 / SAMPLE_RATE as f32;
            let white = noise.next();
            // First difference tilts the noise toward the top octaves.
            let bright = white - previous;
            previous = white;
            0.25 * bright * (-t / 0.012).exp()
        })
        .collect()
}

/// A kick on every beat and a hat on every eighth note.
fn beat(bpm: f32, seconds: f32) -> Vec<f32> {
    let mut out = vec![0.0; sample_count(seconds)];
    let eighth_samples = 60.0 / bpm / 2.0 * SAMPLE_RATE as f32;
    let (kick, hat) = (kick(), hat());
    let mut add = |sound: &[f32], start: usize| {
        for (sample, value) in out.iter_mut().skip(start).zip(sound) {
            *sample += value;
        }
    };
    let eighths = (sample_count(seconds) as f32 / eighth_samples).ceil() as usize;
    for eighth in 0..eighths {
        let start = (eighth as f32 * eighth_samples).round() as usize;
        if eighth % 2 == 0 {
            add(&kick, start);
        }
        add(&hat, start);
    }
    out
}

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
