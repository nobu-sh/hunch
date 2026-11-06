use std::f32::consts::PI;

use super::*;
use crate::test_audio::beat;

const SAMPLE_RATE: u32 = 22_050;
/// Window boundaries snap to the segmenter's ~50 ms frames, and a frame only partly filled with
/// sound already counts as loud.
const TIMING_TOLERANCE_SECONDS: f32 = 0.051;

fn sample_count(seconds: f32) -> usize {
    (seconds * SAMPLE_RATE as f32) as usize
}

fn silence(seconds: f32) -> Vec<f32> {
    vec![0.0; sample_count(seconds)]
}

fn tone(seconds: f32) -> Vec<f32> {
    (0..sample_count(seconds))
        .map(|i| 0.5 * (2.0 * PI * 440.0 * i as f32 / SAMPLE_RATE as f32).sin())
        .collect()
}

/// Deterministic white noise whose RMS level is `db` dBFS.
fn noise(seconds: f32, db: f32) -> Vec<f32> {
    // Uniform noise in [-1, 1] has an RMS of 1 / sqrt(3).
    let gain = 10f32.powf(db / 20.0) * 3f32.sqrt();
    let mut state = 1u32;
    (0..sample_count(seconds))
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            gain * ((state >> 8) as f32 / (1u32 << 23) as f32 - 1.0)
        })
        .collect()
}

/// A decaying 1 kHz click on every beat, long enough to keep the level up between clicks.
fn click_beat(bpm: f32, seconds: f32) -> Vec<f32> {
    let beat_samples = 60.0 / bpm * SAMPLE_RATE as f32;
    (0..sample_count(seconds))
        .map(|i| {
            let since_beat = (i as f32 % beat_samples) / SAMPLE_RATE as f32;
            0.8 * (-since_beat / 0.08).exp() * (2.0 * PI * 1_000.0 * since_beat).sin()
        })
        .collect()
}

fn run(signal: &[f32], chunk_size: usize) -> Vec<SessionEvent> {
    let mut session = Session::new(SAMPLE_RATE);
    signal
        .chunks(chunk_size)
        .flat_map(|chunk| session.push(chunk))
        .collect()
}

struct Window {
    id: u32,
    at_seconds: f32,
    start_reason: WindowStartReason,
    estimates: Vec<Estimate>,
    estimate: Option<Estimate>,
    duration_seconds: f32,
    too_short: bool,
    end_reason: Option<WindowEndReason>,
}

/// Collects ended windows, checking that every event belongs to the window that's open.
fn ended_windows(events: &[SessionEvent]) -> Vec<Window> {
    let mut windows = Vec::new();
    let mut open: Option<Window> = None;
    for event in events {
        match event {
            SessionEvent::WindowStarted {
                window_id,
                at_seconds,
                reason,
            } => {
                assert!(open.is_none(), "window {window_id} started inside another");
                open = Some(Window {
                    id: *window_id,
                    at_seconds: *at_seconds,
                    start_reason: *reason,
                    estimates: Vec::new(),
                    estimate: None,
                    duration_seconds: 0.0,
                    too_short: false,
                    end_reason: None,
                });
            }
            SessionEvent::Estimate {
                window_id,
                estimate,
            } => {
                let window = open.as_mut().expect("estimate outside a window");
                assert_eq!(window.id, *window_id);
                window.estimates.push(estimate.clone());
            }
            SessionEvent::WindowEnded {
                window_id,
                estimate,
                duration_seconds,
                too_short,
                reason,
            } => {
                let mut window = open.take().expect("end outside a window");
                assert_eq!(window.id, *window_id);
                window.estimate = estimate.clone();
                window.duration_seconds = *duration_seconds;
                window.too_short = *too_short;
                window.end_reason = Some(*reason);
                windows.push(window);
            }
        }
    }
    windows
}

fn reasons(window: &Window) -> (WindowStartReason, Option<WindowEndReason>) {
    (window.start_reason, window.end_reason)
}

/// Within `tolerance` of `bpm`, or of half or double it.
fn is_bpm_or_octave(estimate: &Estimate, bpm: f32) -> bool {
    let tolerance = 3.0;
    estimate.bpm.is_some_and(|actual| {
        [bpm / 2.0, bpm, bpm * 2.0]
            .iter()
            .any(|target| (actual - target).abs() <= tolerance)
    })
}

fn assert_near(actual: f32, expected: f32, what: &str) {
    assert!(
        (actual - expected).abs() <= TIMING_TOLERANCE_SECONDS,
        "{what}: expected {expected}, got {actual}"
    );
}

fn final_estimate(window: &Window) -> &Estimate {
    window.estimate.as_ref().expect("a final estimate")
}

fn end_seconds(window: &Window) -> f32 {
    window.at_seconds + window.duration_seconds
}

/// Exactly the window's audio was analyzed: none of the silence that ended it.
fn assert_analyzed_whole_window(window: &Window) {
    let analyzed = final_estimate(window).seconds_analyzed;
    assert!(
        (analyzed - window.duration_seconds).abs() < 1e-3,
        "analyzed {analyzed} s of a {} s window",
        window.duration_seconds
    );
}

#[test]
fn tone_silence_tone_gives_two_windows() {
    let signal = [tone(3.0), silence(3.0), tone(5.0), silence(2.0)].concat();
    let windows = ended_windows(&run(&signal, 2_205));

    assert_eq!(windows.len(), 2);
    let [first, second] = &windows[..] else {
        unreachable!()
    };
    assert_eq!((first.id, second.id), (1, 2));
    assert_near(first.at_seconds, 0.0, "first start");
    assert_near(end_seconds(first), 3.0, "first end");
    assert!(first.too_short);
    assert_near(second.at_seconds, 6.0, "second start");
    assert_near(end_seconds(second), 11.0, "second end");
    assert!(!second.too_short);
    assert!((9..=10).contains(&second.estimates.len()));
    let silence_window = (WindowStartReason::Sound, Some(WindowEndReason::Silence));
    assert_eq!(reasons(first), silence_window);
    assert_eq!(reasons(second), silence_window);
}

#[test]
fn short_dip_does_not_split_a_window() {
    let signal = [tone(3.0), silence(0.2), tone(3.0), silence(2.0)].concat();
    let windows = ended_windows(&run(&signal, 2_205));

    assert_eq!(windows.len(), 1);
    assert_near(end_seconds(&windows[0]), 6.2, "end");
    // The dip is analyzed once the sound comes back.
    assert_analyzed_whole_window(&windows[0]);
}

#[test]
fn short_blip_is_too_short() {
    let signal = [silence(1.0), tone(2.0), silence(2.0)].concat();
    let windows = ended_windows(&run(&signal, 2_205));

    assert_eq!(windows.len(), 1);
    assert_near(windows[0].at_seconds, 1.0, "start");
    assert_near(end_seconds(&windows[0]), 3.0, "end");
    assert!(windows[0].too_short);
}

#[test]
fn quiet_noise_never_opens_a_window() {
    let signal = noise(10.0, -60.0);
    assert!(run(&signal, 2_205).is_empty());
}

#[test]
fn chunked_pushes_match_one_push() {
    let signal = [
        silence(0.5),
        tone(2.0),
        silence(0.3),
        click_beat(120.0, 3.0),
        silence(2.0),
        tone(1.0),
    ]
    .concat();
    let whole = run(&signal, signal.len());
    assert!(whole.len() > 3, "{whole:?}");
    for chunk_size in [1_000, 2_205, 4_097] {
        assert_eq!(run(&signal, chunk_size), whole, "chunk size {chunk_size}");
    }
}

#[test]
fn analyzer_starts_fresh_in_each_window() {
    let signal = [
        click_beat(90.0, 10.0),
        silence(3.0),
        click_beat(140.0, 10.0),
        silence(2.0),
    ]
    .concat();
    let windows = ended_windows(&run(&signal, 2_205));

    assert_eq!(windows.len(), 2);
    assert_analyzed_whole_window(&windows[1]);
    let bpm = final_estimate(&windows[1])
        .bpm
        .expect("a tempo for a steady beat");
    let is_140_or_octave = [70.0, 140.0, 280.0]
        .iter()
        .any(|target| (bpm - target).abs() <= 2.0);
    assert!(is_140_or_octave, "second window bpm {bpm}");
}

#[test]
fn trailing_silence_is_not_analyzed() {
    let signal = [tone(5.0), silence(3.0)].concat();
    let windows = ended_windows(&run(&signal, 2_205));

    assert_eq!(windows.len(), 1);
    assert_near(end_seconds(&windows[0]), 5.0, "end");
    assert_analyzed_whole_window(&windows[0]);
}

#[test]
fn estimates_report_tuning_only_with_a_key() {
    let signal = [tone(6.0), silence(2.0)].concat();
    let windows = ended_windows(&run(&signal, 2_205));
    let estimate = final_estimate(&windows[0]);
    assert!(estimate.key.is_some(), "{estimate:?}");
    let tuning = estimate.tuning_cents.expect("tuning with a key");
    assert!(tuning.abs() < 5.0, "A = 440 tone tuned {tuning} cents off");

    let hiss = [noise(6.0, -20.0), silence(2.0)].concat();
    let windows = ended_windows(&run(&hiss, 2_205));
    let estimate = final_estimate(&windows[0]);
    assert_eq!(estimate.key, None);
    assert_eq!(estimate.tuning_cents, None);
}

#[test]
fn reset_drops_the_open_window_and_keeps_counting() {
    let mut session = Session::new(SAMPLE_RATE);
    session.push(&tone(2.0));
    session.reset();
    let events = session.push(&[silence(1.0), tone(5.0), silence(2.0)].concat());

    let windows = ended_windows(&events);
    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0].id, 2);
    assert_near(windows[0].at_seconds, 3.0, "start");
}

#[test]
fn finish_ends_the_open_window_with_its_final_estimate() {
    let mut session = Session::new(SAMPLE_RATE);
    let mut events = session.push(&[silence(1.0), tone(5.0)].concat());
    events.extend(session.finish());

    let windows = ended_windows(&events);
    assert_eq!(windows.len(), 1);
    assert_near(windows[0].at_seconds, 1.0, "start");
    assert_near(end_seconds(&windows[0]), 6.0, "end");
    assert!(!windows[0].too_short);
    assert_analyzed_whole_window(&windows[0]);
    assert_eq!(windows[0].end_reason, Some(WindowEndReason::Finished));
}

#[test]
fn finish_marks_a_short_window_too_short() {
    let mut session = Session::new(SAMPLE_RATE);
    let mut events = session.push(&tone(2.0));
    events.extend(session.finish());

    let windows = ended_windows(&events);
    assert_eq!(windows.len(), 1);
    assert!(windows[0].too_short);
}

#[test]
fn finish_leaves_out_a_trailing_quiet_stretch() {
    let mut session = Session::new(SAMPLE_RATE);
    let mut events = session.push(&[tone(5.0), silence(1.0)].concat());
    assert!(ended_windows(&events).is_empty(), "1 s of silence ended it");
    events.extend(session.finish());

    let windows = ended_windows(&events);
    assert_eq!(windows.len(), 1);
    assert_near(end_seconds(&windows[0]), 5.0, "end");
    assert_analyzed_whole_window(&windows[0]);
}

#[test]
fn finish_without_an_open_window_ends_nothing() {
    let mut session = Session::new(SAMPLE_RATE);
    session.push(&[tone(5.0), silence(2.0)].concat());

    assert!(session.finish().is_empty());
}

#[test]
fn a_finished_session_keeps_counting() {
    let mut session = Session::new(SAMPLE_RATE);
    session.push(&tone(5.0));
    session.finish();
    let events = session.push(&[silence(1.0), tone(5.0), silence(2.0)].concat());

    let windows = ended_windows(&events);
    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0].id, 2);
    assert_near(windows[0].at_seconds, 6.0, "start");
}

/// 30 s at 120 BPM straight into 30 s at 140 BPM, then silence.
fn tempo_step() -> Vec<f32> {
    [beat(120.0, 30.0), beat(140.0, 30.0), silence(2.0)].concat()
}

#[test]
fn a_tempo_step_splits_the_window_at_the_step() {
    let windows = ended_windows(&run(&tempo_step(), 2_205));

    assert_eq!(windows.len(), 2);
    let [old, new] = &windows[..] else {
        unreachable!()
    };
    assert_eq!((old.id, new.id), (1, 2));
    assert_eq!(
        reasons(old),
        (WindowStartReason::Sound, Some(WindowEndReason::Change))
    );
    assert_eq!(
        reasons(new),
        (WindowStartReason::Change, Some(WindowEndReason::Silence))
    );
    assert!(
        (end_seconds(old) - 30.0).abs() <= 1.0,
        "change at {}",
        end_seconds(old)
    );
    assert!(!old.too_short);
    assert!(
        is_bpm_or_octave(final_estimate(old), 120.0),
        "{:?}",
        old.estimate
    );
    assert!(
        is_bpm_or_octave(final_estimate(new), 140.0),
        "{:?}",
        new.estimate
    );
}

#[test]
fn a_split_leaves_no_gap_or_overlap() {
    let windows = ended_windows(&run(&tempo_step(), 2_205));
    let [old, new] = &windows[..] else {
        panic!("expected two windows, got {}", windows.len())
    };

    // The new window starts on the sample where the old one ended.
    assert_eq!(new.at_seconds, end_seconds(old));
    // The old window's estimate is the one at the change point, of exactly its audio.
    assert_analyzed_whole_window(old);
    // The new window was analyzed from the change point: every half second of it, once.
    assert_analyzed_whole_window(new);
    let expected_estimates = (new.duration_seconds / 0.5).floor() as usize;
    assert_eq!(new.estimates.len(), expected_estimates);
    for (index, estimate) in new.estimates.iter().enumerate() {
        let expected_seconds = (index + 1) as f32 * 0.5;
        assert!(
            (estimate.seconds_analyzed - expected_seconds).abs() < 1e-3,
            "estimate {index} at {} s",
            estimate.seconds_analyzed
        );
    }
}

#[test]
fn chunked_pushes_split_like_one_push() {
    let signal = tempo_step();
    let whole = run(&signal, signal.len());
    assert_eq!(ended_windows(&whole).len(), 2);
    for chunk_size in [1_000, 4_097, 22_050] {
        assert_eq!(run(&signal, chunk_size), whole, "chunk size {chunk_size}");
    }
}

#[test]
fn a_steady_beat_never_splits() {
    let signal = [beat(128.0, 90.0), silence(2.0)].concat();
    let windows = ended_windows(&run(&signal, 2_205));

    assert_eq!(windows.len(), 1);
    assert_eq!(
        reasons(&windows[0]),
        (WindowStartReason::Sound, Some(WindowEndReason::Silence))
    );
}

#[test]
fn window_ids_keep_counting_after_a_split() {
    let signal = [tempo_step(), beat(120.0, 10.0), silence(2.0)].concat();
    let windows = ended_windows(&run(&signal, 2_205));

    let ids: Vec<u32> = windows.iter().map(|window| window.id).collect();
    assert_eq!(ids, [1, 2, 3]);
    assert_eq!(
        reasons(&windows[2]),
        (WindowStartReason::Sound, Some(WindowEndReason::Silence))
    );
}

#[test]
fn silence_resets_the_change_detector() {
    // Without the reset, the second window's 140 BPM would read as a change from the first's 120.
    let signal = [
        beat(120.0, 30.0),
        silence(3.0),
        beat(140.0, 30.0),
        silence(2.0),
    ]
    .concat();
    let windows = ended_windows(&run(&signal, 2_205));

    assert_eq!(windows.len(), 2);
    for window in &windows {
        assert_eq!(
            reasons(window),
            (WindowStartReason::Sound, Some(WindowEndReason::Silence))
        );
    }
}

#[test]
fn finish_after_a_split_ends_the_new_window() {
    let mut session = Session::new(SAMPLE_RATE);
    let mut events = session.push(&[beat(120.0, 30.0), beat(140.0, 30.0)].concat());
    events.extend(session.finish());

    let windows = ended_windows(&events);
    assert_eq!(windows.len(), 2);
    assert_eq!(
        reasons(&windows[1]),
        (WindowStartReason::Change, Some(WindowEndReason::Finished))
    );
    assert_analyzed_whole_window(&windows[1]);
}

/// 120 BPM fading into 90 BPM over a minute, then silence.
fn slow_crossfade() -> Vec<f32> {
    const SECONDS: f32 = 60.0;
    let (old, new) = (beat(120.0, SECONDS), beat(90.0, SECONDS));
    let length = old.len() as f32;
    let mut signal: Vec<f32> = old
        .iter()
        .zip(&new)
        .enumerate()
        .map(|(index, (old, new))| {
            let gain = index as f32 / length;
            (1.0 - gain) * old + gain * new
        })
        .collect();
    signal.extend(silence(2.0));
    signal
}

#[test]
fn a_slow_crossfade_splits_once() {
    let windows = ended_windows(&run(&slow_crossfade(), 2_205));

    let durations: Vec<f32> = windows
        .iter()
        .map(|window| window.duration_seconds)
        .collect();
    assert_eq!(windows.len(), 2, "window durations {durations:?}");
    assert_eq!(
        reasons(&windows[1]),
        (WindowStartReason::Change, Some(WindowEndReason::Silence))
    );
}
