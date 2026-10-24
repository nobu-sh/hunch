use std::f32::consts::PI;

use super::*;

const SAMPLE_RATE: u32 = 22_050;
const STEPS_PER_BAR: usize = 16;

/// Deterministic white noise in [-1, 1].
struct Noise(u32);

impl Noise {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.0 >> 8) as f32 / (1u32 << 23) as f32 - 1.0
    }
}

#[derive(Clone, Copy)]
enum Drum {
    Kick,
    Snare,
    Hat,
}

impl Drum {
    fn render(self, noise: &mut Noise) -> Vec<f32> {
        let seconds = match self {
            Drum::Kick => 0.3,
            Drum::Snare => 0.2,
            Drum::Hat => 0.05,
        };
        let length = (seconds * SAMPLE_RATE as f32) as usize;
        let mut phase = 0.0;
        let mut previous_noise = 0.0;
        (0..length)
            .map(|i| {
                let t = i as f32 / SAMPLE_RATE as f32;
                let white = noise.next();
                match self {
                    Drum::Kick => {
                        let frequency = 50.0 + 70.0 * (-t / 0.03).exp();
                        phase += 2.0 * PI * frequency / SAMPLE_RATE as f32;
                        0.9 * phase.sin() * (-t / 0.12).exp()
                    }
                    Drum::Snare => {
                        let tone = (2.0 * PI * 190.0 * t).sin();
                        (0.4 * tone + 0.5 * white) * (-t / 0.06).exp()
                    }
                    Drum::Hat => {
                        // First difference tilts the noise toward the top octaves.
                        let bright = white - previous_noise;
                        previous_noise = white;
                        0.25 * bright * (-t / 0.012).exp()
                    }
                }
            })
            .collect()
    }
}

/// A one-bar pattern of 16th-note steps, plus extra hats at arbitrary fractions of a step.
struct Pattern {
    kicks: &'static [usize],
    snares: &'static [usize],
    hats: &'static [usize],
    /// Hat roll hits, in 16th steps, only in every other bar.
    roll: &'static [f32],
}

const BOOM_BAP: Pattern = Pattern {
    kicks: &[0, 8],
    snares: &[4, 12],
    hats: &[0, 2, 4, 6, 8, 10, 12, 14],
    roll: &[],
};

const TRAP: Pattern = Pattern {
    kicks: &[0, 7, 10],
    snares: &[8],
    hats: &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
    roll: &[12.5, 13.5, 14.333, 14.667, 15.333, 15.667],
};

fn render(pattern: &Pattern, bpm: f32, seconds: f32) -> Vec<f32> {
    let mut noise = Noise(7);
    let kick = Drum::Kick.render(&mut noise);
    let snare = Drum::Snare.render(&mut noise);
    let hat = Drum::Hat.render(&mut noise);

    let mut out = vec![0.0; (seconds * SAMPLE_RATE as f32) as usize];
    let step_samples = 60.0 / bpm / 4.0 * SAMPLE_RATE as f32;
    let bar_count = (out.len() as f32 / (step_samples * STEPS_PER_BAR as f32)).ceil() as usize;
    let mut add = |sound: &[f32], step: f32| {
        let start = (step * step_samples).round() as usize;
        for (sample, value) in out.iter_mut().skip(start).zip(sound) {
            *sample += value;
        }
    };

    for bar in 0..bar_count {
        let bar_step = (bar * STEPS_PER_BAR) as f32;
        for &step in pattern.kicks {
            add(&kick, bar_step + step as f32);
        }
        for &step in pattern.snares {
            add(&snare, bar_step + step as f32);
        }
        for &step in pattern.hats {
            add(&hat, bar_step + step as f32);
        }
        if bar % 2 == 1 {
            for &step in pattern.roll {
                add(&hat, bar_step + step);
            }
        }
    }
    out
}

fn detect(samples: &[f32]) -> TempoDetector {
    let mut detector = TempoDetector::new(SAMPLE_RATE);
    detector.push(samples);
    detector
}

fn assert_tempo(pattern: &Pattern, target: f32) {
    let detector = detect(&render(pattern, target, 15.0));
    let estimate = detector.estimate();
    let bpm = estimate.bpm.expect("a tempo for a steady beat");
    assert!(
        (bpm - target).abs() <= 1.0,
        "target {target}, got {estimate:?}, candidates {:?}",
        detector.octave_candidates()
    );
}

#[test]
fn boom_bap_at_90() {
    assert_tempo(&BOOM_BAP, 90.0);
}

#[test]
fn boom_bap_at_120() {
    assert_tempo(&BOOM_BAP, 120.0);
}

#[test]
fn boom_bap_at_140() {
    assert_tempo(&BOOM_BAP, 140.0);
}

#[test]
fn trap_at_140_is_labeled_140_not_70() {
    assert_tempo(&TRAP, 140.0);
}

#[test]
fn candidates_include_half_and_double_with_finite_features() {
    let candidates = detect(&render(&BOOM_BAP, 120.0, 15.0)).octave_candidates();
    let has_tempo = |target: f32| candidates.iter().any(|c| (c.bpm - target).abs() < 1.5);
    assert!(has_tempo(60.0) && has_tempo(240.0), "{candidates:?}");

    for candidate in &candidates {
        assert_eq!(candidate.features.len(), OCTAVE_FEATURE_NAMES.len());
        assert!(
            candidate.features.iter().all(|value| value.is_finite()),
            "{candidate:?}"
        );
    }
}

#[test]
fn loop_tempi_have_power_of_two_beats_per_loop() {
    let two_second_loop = Repetition {
        lag: 200.0,
        strength: 0.99,
    };
    assert_eq!(loop_tempi(two_second_loop, 100.0), vec![60.0, 120.0, 240.0]);

    let weak = Repetition {
        strength: 0.3,
        ..two_second_loop
    };
    assert!(loop_tempi(weak, 100.0).is_empty());
}

#[test]
fn loop_tempi_lead_the_candidates() {
    let three_quarter_pulse = Period {
        lag: 100.0 * 60.0 / 90.0,
        strength: 0.8,
    };
    let tempi = candidate_tempi(CandidateSources {
        periods: &[three_quarter_pulse],
        repetition: Some(Repetition {
            lag: 200.0,
            strength: 0.99,
        }),
        frame_rate: 100.0,
    });
    assert!(tempi.contains(&120.0), "{tempi:?}");
    assert!(
        tempi.iter().any(|&bpm| (bpm - 90.0).abs() < 0.01),
        "{tempi:?}"
    );
}

#[test]
fn alternate_is_the_likeliest_octave_relative() {
    let ranked = |pairs: &[(f32, f32)]| -> Vec<Ranked> {
        pairs
            .iter()
            .map(|&(bpm, probability)| Ranked { bpm, probability })
            .collect()
    };
    let close = ranked(&[(140.0, 0.4), (105.0, 0.3), (70.0, 0.2), (280.0, 0.1)]);
    assert_eq!(octave_alternate(&close), Some(70.0));

    let sure = ranked(&[(140.0, 0.8), (70.0, 0.1), (105.0, 0.1)]);
    assert_eq!(octave_alternate(&sure), None);
}

#[test]
fn confidence_rises_with_probability() {
    let low = calibrated_confidence(0.3);
    let high = calibrated_confidence(0.9);
    assert!((0.0..=1.0).contains(&low) && (0.0..=1.0).contains(&high));
    assert!(high > low, "{low} {high}");
}

#[test]
fn feature_names_match_weights() {
    assert_eq!(
        OCTAVE_FEATURE_NAMES.len(),
        octave_weights::FEATURE_MEANS.len()
    );
    assert_eq!(
        OCTAVE_FEATURE_NAMES.len(),
        octave_weights::FEATURE_STDS.len()
    );
    assert!(octave_weights::FEATURE_STDS.iter().all(|&std| std > 0.0));
}

#[test]
fn no_tempo_before_four_seconds() {
    let detector = detect(&render(&BOOM_BAP, 120.0, 3.5));
    assert_eq!(detector.estimate(), TempoEstimate::NONE);
    assert!(detector.octave_candidates().is_empty());
}

#[test]
fn silence_has_no_tempo() {
    let detector = detect(&vec![0.0; SAMPLE_RATE as usize * 10]);
    assert_eq!(detector.estimate().bpm, None);
}

#[test]
fn white_noise_has_no_confident_tempo() {
    let mut noise = Noise(42);
    let samples: Vec<f32> = (0..SAMPLE_RATE as usize * 15)
        .map(|_| 0.5 * noise.next())
        .collect();
    let estimate = detect(&samples).estimate();
    assert!(
        estimate.bpm.is_none() || estimate.confidence < 0.2,
        "{estimate:?}"
    );
}

#[test]
fn chunked_push_matches_single_push() {
    let samples = render(&BOOM_BAP, 100.0, 8.0);
    let whole = detect(&samples);

    let mut chunked = TempoDetector::new(SAMPLE_RATE);
    for chunk in samples.chunks(2_205) {
        chunked.push(chunk);
    }
    assert_eq!(chunked.estimate(), whole.estimate());
    assert_eq!(chunked.octave_candidates(), whole.octave_candidates());
}

#[test]
fn reset_clears_history() {
    let mut detector = detect(&render(&BOOM_BAP, 120.0, 8.0));
    assert!(detector.estimate().bpm.is_some());

    detector.reset();
    assert_eq!(detector.estimate(), TempoEstimate::NONE);
    detector.push(&vec![0.0; SAMPLE_RATE as usize * 8]);
    assert_eq!(detector.estimate().bpm, None);
}

#[test]
fn works_at_44100() {
    let mut detector = TempoDetector::new(44_100);
    let at_22050 = render(&BOOM_BAP, 120.0, 15.0);
    let upsampled: Vec<f32> = at_22050.iter().flat_map(|&s| [s, s]).collect();
    detector.push(&upsampled);
    let bpm = detector.estimate().bpm.expect("a tempo at 44.1 kHz");
    assert!((bpm - 120.0).abs() <= 1.0, "{bpm}");
}
