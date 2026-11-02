use hunch_core::{Estimate, Session, SessionEvent};

use super::*;

const LISTEN_SECONDS: usize = 12;

fn last_estimate(samples: &[f32]) -> Option<Estimate> {
    let mut session = Session::new(SAMPLE_RATE);
    let chunk_len = (CHUNK_SECONDS * SAMPLE_RATE as f32) as usize;
    let mut last = None;
    for chunk in samples.chunks(chunk_len) {
        for event in session.push(chunk) {
            if let SessionEvent::Estimate { estimate, .. } = event {
                last = Some(estimate);
            }
        }
    }
    last
}

#[test]
fn the_loop_is_sixteen_bars_long() {
    let expected = (LOOP_BARS as f32 * BAR_SECONDS * SAMPLE_RATE as f32).ceil() as usize;
    assert_eq!(render(SAMPLE_RATE).len(), expected);
}

#[test]
fn the_core_hears_120_bpm_in_a_minor() {
    let samples = render(SAMPLE_RATE);
    let listened = &samples[..LISTEN_SECONDS * SAMPLE_RATE as usize];
    let estimate = last_estimate(listened).expect("the beat produces an estimate");

    let bpm = estimate.bpm.expect("a tempo");
    let is_120_or_an_octave = [60.0, 120.0, 240.0]
        .iter()
        .any(|target| (bpm - target).abs() < 2.0);
    assert!(is_120_or_an_octave, "bpm {bpm}");

    let key = estimate.key.expect("a key").to_string();
    assert!(["A minor", "C major"].contains(&key.as_str()), "key {key}");
}
