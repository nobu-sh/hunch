use std::f32::consts::PI;

use super::*;
use crate::{Mode, PitchClass};

const SAMPLE_RATE: u32 = 22_050;
const CHORD_SECONDS: f32 = 1.5;
const HARMONICS: usize = 6;

const C: u8 = 0;
const F_SHARP: u8 = 6;
const A: u8 = 9;

/// MIDI notes per chord; the first note is the bass. The minor progression uses the natural
/// minor v chord so the parallel major's leading tone doesn't make it ambiguous.
type Progression = [&'static [u8]];

const C_MAJOR: &Progression = &[
    &[36, 60, 64, 67],
    &[41, 60, 65, 69],
    &[43, 59, 62, 67],
    &[36, 60, 64, 67, 72],
];
const A_MINOR: &Progression = &[
    &[45, 57, 60, 64],
    &[38, 57, 62, 65],
    &[40, 55, 59, 64],
    &[45, 57, 60, 64, 69],
];
const F_SHARP_MAJOR: &Progression = &[
    &[42, 66, 70, 73],
    &[47, 66, 71, 75],
    &[37, 65, 68, 73],
    &[42, 66, 70, 73, 78],
];

fn key(tonic: u8, mode: Mode) -> Key {
    let tonic = PitchClass::new(tonic).expect("valid pitch class");
    Key { tonic, mode }
}

fn midi_hz(midi: f32) -> f32 {
    440.0 * 2f32.powf((midi - 69.0) / 12.0)
}

/// Harmonic tone with 1/h partials, added into `out` with a short fade in and out.
fn add_tone(out: &mut [f32], midi: f32, amplitude: f32) {
    let fundamental = midi_hz(midi);
    let fade = (0.01 * SAMPLE_RATE as f32) as usize;
    let len = out.len();
    for (i, sample) in out.iter_mut().enumerate() {
        let envelope = (i.min(len - 1 - i) as f32 / fade as f32).min(1.0);
        let t = i as f32 / SAMPLE_RATE as f32;
        let tone: f32 = (1..=HARMONICS)
            .map(|h| (2.0 * PI * fundamental * h as f32 * t).sin() / h as f32)
            .sum();
        *sample += amplitude * envelope * tone;
    }
}

/// Plays the progression twice, every note shifted by `detune_cents`.
fn progression(chords: &Progression, detune_cents: f32) -> Vec<f32> {
    let chord_len = (CHORD_SECONDS * SAMPLE_RATE as f32) as usize;
    let mut out = Vec::new();
    for chord in chords.iter().chain(chords) {
        let mut segment = vec![0.0; chord_len];
        for (index, &note) in chord.iter().enumerate() {
            let amplitude = if index == 0 { 0.25 } else { 0.1 };
            add_tone(
                &mut segment,
                f32::from(note) + detune_cents / 100.0,
                amplitude,
            );
        }
        out.extend(segment);
    }
    out
}

/// Deterministic xorshift noise in [-1, 1].
fn noise(len: usize) -> Vec<f32> {
    let mut state: u32 = 0x9E37_79B9;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state as f32 / u32::MAX as f32 * 2.0 - 1.0
        })
        .collect()
}

/// Four-on-the-floor kicks (pitch-swept sines) with noise-burst hats and snares, at 120 BPM.
fn drum_loop(seconds: f32) -> Vec<f32> {
    let len = (seconds * SAMPLE_RATE as f32) as usize;
    let beat = SAMPLE_RATE as usize / 2;
    let hiss = noise(len);
    let mut out = vec![0.0; len];
    for (i, sample) in out.iter_mut().enumerate() {
        let beat_index = i / beat;
        let since_beat = (i % beat) as f32 / SAMPLE_RATE as f32;
        let since_eighth = ((i % (beat / 2)) as f32) / SAMPLE_RATE as f32;

        // Sweeps from 150 Hz down to 50 Hz: the integral of 50 + 100 * exp(-t / 0.03).
        let kick_phase = 2.0 * PI * (50.0 * since_beat + 3.0 * (1.0 - (-since_beat / 0.03).exp()));
        let kick = 0.8 * (-since_beat / 0.15).exp() * kick_phase.sin();
        let snare_gain = if beat_index % 2 == 1 { 0.5 } else { 0.0 };
        let snare = snare_gain * (-since_beat / 0.08).exp() * hiss[i];
        let hat = 0.2 * (-since_eighth / 0.02).exp() * hiss[(i * 7) % len];
        *sample = kick + snare + hat;
    }
    out
}

fn detect(samples: &[f32]) -> KeyDetector {
    let mut detector = KeyDetector::new(SAMPLE_RATE);
    detector.push(samples);
    detector
}

fn assert_key(samples: &[f32], expected: Key) {
    let detector = detect(samples);
    let estimate = detector.estimate();
    assert_eq!(
        estimate.key,
        Some(expected),
        "{estimate:?}, chroma {:?}",
        detector.chroma()
    );
    assert!(estimate.confidence > 0.0, "{estimate:?}");
}

#[test]
fn detects_c_major() {
    assert_key(&progression(C_MAJOR, 0.0), key(C, Mode::Major));
}

#[test]
fn detects_a_minor() {
    assert_key(&progression(A_MINOR, 0.0), key(A, Mode::Minor));
}

#[test]
fn detects_f_sharp_major() {
    assert_key(&progression(F_SHARP_MAJOR, 0.0), key(F_SHARP, Mode::Major));
}

#[test]
fn detects_key_and_tuning_of_detuned_audio() {
    for detune in [35.0, -40.0] {
        for (chords, expected) in [
            (C_MAJOR, key(C, Mode::Major)),
            (A_MINOR, key(A, Mode::Minor)),
            (F_SHARP_MAJOR, key(F_SHARP, Mode::Major)),
        ] {
            let detector = detect(&progression(chords, detune));
            let tuning = detector.tuning_cents();
            assert!((tuning - detune).abs() <= 10.0, "{detune} -> {tuning}");
            assert_eq!(detector.estimate().key, Some(expected), "detune {detune}");
        }
    }
}

#[test]
fn in_tune_audio_has_near_zero_tuning() {
    let tuning = detect(&progression(C_MAJOR, 0.0)).tuning_cents();
    assert!(tuning.abs() <= 5.0, "{tuning}");
}

#[test]
fn white_noise_has_no_key() {
    let samples: Vec<f32> = noise(10 * SAMPLE_RATE as usize)
        .into_iter()
        .map(|s| 0.3 * s)
        .collect();
    let estimate = detect(&samples).estimate();
    assert_eq!(estimate.key, None, "{estimate:?}");
    assert_eq!(estimate.confidence, 0.0);
}

#[test]
fn drum_loop_has_no_key() {
    let estimate = detect(&drum_loop(8.0)).estimate();
    assert_eq!(estimate.key, None, "{estimate:?}");
    assert_eq!(estimate.confidence, 0.0);
}

#[test]
fn tonal_audio_is_more_tonal_than_drums() {
    let tonal = detect(&progression(C_MAJOR, 0.0)).estimate().tonalness;
    let drums = detect(&drum_loop(8.0)).estimate().tonalness;
    assert!(tonal > 0.5, "{tonal}");
    assert!(drums < tonal / 2.0, "{drums} vs {tonal}");
}

#[test]
fn silence_has_no_key() {
    let detector = detect(&vec![0.0; 3 * SAMPLE_RATE as usize]);
    let estimate = detector.estimate();
    assert_eq!(estimate.key, None);
    assert_eq!(estimate.tonalness, 0.0);
    assert_eq!(detector.chroma(), [0.0; PITCH_CLASSES]);
    assert_eq!(detector.tuning_cents(), 0.0);
}

#[test]
fn chunked_push_matches_single_push() {
    let samples = progression(A_MINOR, 20.0);
    let whole = detect(&samples);

    let mut chunked = KeyDetector::new(SAMPLE_RATE);
    let mut rest = samples.as_slice();
    for size in [1, 777, 2205, 4097, 13].iter().cycle() {
        if rest.is_empty() {
            break;
        }
        let (chunk, tail) = rest.split_at((*size).min(rest.len()));
        chunked.push(chunk);
        rest = tail;
    }

    assert_eq!(chunked.estimate(), whole.estimate());
    assert_eq!(chunked.chroma(), whole.chroma());
    assert_eq!(chunked.tuning_cents(), whole.tuning_cents());
}

#[test]
fn reset_clears_evidence() {
    let mut detector = detect(&progression(C_MAJOR, 0.0));
    detector.reset();
    let estimate = detector.estimate();
    assert_eq!(estimate.key, None);
    assert_eq!(estimate.confidence, 0.0);
    assert_eq!(estimate.tonalness, 0.0);

    detector.push(&progression(A_MINOR, 0.0));
    assert_eq!(detector.estimate().key, Some(key(A, Mode::Minor)));
}

#[test]
fn confidence_grows_with_evidence() {
    let samples = progression(C_MAJOR, 0.0);
    let short = detect(&samples[..2 * SAMPLE_RATE as usize]).estimate();
    let long = detect(&samples).estimate();
    assert!(long.confidence > short.confidence, "{short:?} vs {long:?}");
}

#[test]
fn handles_other_sample_rates() {
    let rate = 44_100;
    let chunk_len = (CHORD_SECONDS * rate as f32) as usize;
    let mut samples = Vec::new();
    for chord in C_MAJOR.iter().chain(C_MAJOR) {
        let mut segment = vec![0.0f32; chunk_len];
        for (index, &note) in chord.iter().enumerate() {
            let hz = midi_hz(f32::from(note));
            let amplitude = if index == 0 { 0.25 } else { 0.1 };
            for (i, sample) in segment.iter_mut().enumerate() {
                *sample += amplitude * (2.0 * PI * hz * i as f32 / rate as f32).sin();
            }
        }
        samples.extend(segment);
    }
    let mut detector = KeyDetector::new(rate);
    detector.push(&samples);
    assert_eq!(detector.estimate().key, Some(key(C, Mode::Major)));
}

#[test]
fn every_profile_detects_clear_progressions() {
    for profile in KeyProfile::ALL {
        let config = KeyDetectorConfig {
            profile,
            ..KeyDetectorConfig::default()
        };
        let mut detector = KeyDetector::with_config(SAMPLE_RATE, config);
        detector.push(&progression(A_MINOR, 0.0));
        assert_eq!(
            detector.estimate().key,
            Some(key(A, Mode::Minor)),
            "{profile:?}"
        );
    }
}
