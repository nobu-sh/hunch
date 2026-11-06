use hunch_core::{Estimate, Session, SessionEvent, WindowEndReason};

use super::*;

const LISTEN_SECONDS: usize = 12;

fn session_events(samples: &[f32]) -> Vec<SessionEvent> {
    let mut session = Session::new(SAMPLE_RATE);
    let chunk_len = (CHUNK_SECONDS * SAMPLE_RATE as f32) as usize;
    samples
        .chunks(chunk_len)
        .flat_map(|chunk| session.push(chunk))
        .collect()
}

fn last_estimate(samples: &[f32]) -> Option<Estimate> {
    session_events(samples)
        .into_iter()
        .filter_map(|event| match event {
            SessionEvent::Estimate { estimate, .. } => Some(estimate),
            _ => None,
        })
        .next_back()
}

fn is_bpm_or_octave(bpm: f32, target: f32) -> bool {
    [target / 2.0, target, target * 2.0]
        .iter()
        .any(|octave| (bpm - octave).abs() < 2.0)
}

#[test]
fn the_beat_loop_is_sixteen_bars_long() {
    let expected = (16.0 * TEST_BEAT.bar_seconds() * SAMPLE_RATE as f32).ceil() as usize;
    assert_eq!(render_source(DevSource::TestBeat).len(), expected);
}

#[test]
fn the_core_hears_120_bpm_in_a_minor() {
    let samples = render_source(DevSource::TestBeat);
    let listened = &samples[..LISTEN_SECONDS * SAMPLE_RATE as usize];
    let estimate = last_estimate(listened).expect("the beat produces an estimate");

    let bpm = estimate.bpm.expect("a tempo");
    assert!(is_bpm_or_octave(bpm, 120.0), "bpm {bpm}");

    let key = estimate.key.expect("a key").to_string();
    assert!(["A minor", "C major"].contains(&key.as_str()), "key {key}");
}

#[test]
fn the_core_splits_the_mix_from_120_to_140_bpm() {
    let samples = render_source(DevSource::TestMix);
    let first_song_seconds = TEST_MIX[0].bars as f32 * TEST_MIX[0].bar_seconds();

    let ended: Vec<(Option<Estimate>, f32, WindowEndReason)> = session_events(&samples)
        .into_iter()
        .filter_map(|event| match event {
            SessionEvent::WindowEnded {
                estimate,
                duration_seconds,
                reason,
                ..
            } => Some((estimate, duration_seconds, reason)),
            _ => None,
        })
        .collect();

    let (estimate, duration_seconds, reason) = ended.first().expect("a split in the mix");
    assert_eq!(*reason, WindowEndReason::Change);
    assert!(
        (duration_seconds - first_song_seconds).abs() <= 1.0,
        "split at {duration_seconds} s, the songs change at {first_song_seconds} s"
    );
    let bpm = estimate.as_ref().and_then(|estimate| estimate.bpm);
    assert!(
        bpm.is_some_and(|bpm| is_bpm_or_octave(bpm, 120.0)),
        "old bpm {bpm:?}"
    );

    let new_song = &samples[(first_song_seconds * SAMPLE_RATE as f32) as usize..];
    let new_bpm = last_estimate(new_song).and_then(|estimate| estimate.bpm);
    assert!(
        new_bpm.is_some_and(|bpm| is_bpm_or_octave(bpm, 140.0)),
        "new bpm {new_bpm:?}"
    );
}
