use std::f32::consts::PI;

use super::*;

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
    estimate_count: usize,
    estimate: Option<Estimate>,
    duration_seconds: f32,
    too_short: bool,
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
            } => {
                assert!(open.is_none(), "window {window_id} started inside another");
                open = Some(Window {
                    id: *window_id,
                    at_seconds: *at_seconds,
                    estimate_count: 0,
                    estimate: None,
                    duration_seconds: 0.0,
                    too_short: false,
                });
            }
            SessionEvent::Estimate { window_id, .. } => {
                let window = open.as_mut().expect("estimate outside a window");
                assert_eq!(window.id, *window_id);
                window.estimate_count += 1;
            }
            SessionEvent::WindowEnded {
                window_id,
                estimate,
                duration_seconds,
                too_short,
            } => {
                let mut window = open.take().expect("end outside a window");
                assert_eq!(window.id, *window_id);
                window.estimate = estimate.clone();
                window.duration_seconds = *duration_seconds;
                window.too_short = *too_short;
                windows.push(window);
            }
        }
    }
    windows
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
    assert!((9..=10).contains(&second.estimate_count));
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
