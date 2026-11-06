//! The production change detector (`hunch_core::ChangeDetector`), run the way a session runs it:
//! one analyzer per window, and on a change the window ends at the change point and a fresh
//! analyzer re-analyzes the audio from there. It can't replay a trace, because a split restarts
//! the analyzer, so it runs on the clip's audio.

use hunch_core::{Analyzer, ChangeDetector, Observation};

use super::detect::{Cause, Split};
use super::trace::TICK_SECONDS;
use crate::resample::ANALYSIS_SAMPLE_RATE;
use crate::run;

/// The bench's name for this detector.
pub const CONFIG: &str = "core";

pub fn splits(samples: &[f32]) -> Vec<Split> {
    let rate = ANALYSIS_SAMPLE_RATE as f32;
    let mut analyzer = Analyzer::new(ANALYSIS_SAMPLE_RATE);
    let mut detector = ChangeDetector::new();
    let mut splits = Vec::new();
    let mut window_start = 0;
    // The next sample to analyze, and the furthest one heard: a change found while re-analyzing
    // is detected at the live position.
    let mut position = 0;
    let mut live = 0;
    while position < samples.len() {
        let end = (position + run::chunk_len()).min(samples.len());
        let chunk = &samples[position..end];
        position = end;
        live = live.max(end);
        let Some(estimate) = analyzer.push(chunk) else {
            continue;
        };
        let observation = Observation {
            analyzer: &analyzer,
            estimate: &estimate,
        };
        let Some(change) = detector.update(observation) else {
            continue;
        };
        let change_sample = window_start + (change.at_seconds * rate).round() as usize;
        splits.push(Split {
            detect_tick: tick_at(live as f32 / rate),
            change_tick: tick_at(change_sample as f32 / rate),
            cause: Cause::Tempo,
        });
        analyzer.reset();
        window_start = change_sample;
        position = change_sample;
    }
    splits
}

/// The trace tick whose estimate is at `seconds` (the inverse of `Trace::seconds_of`).
fn tick_at(seconds: f32) -> usize {
    ((seconds / TICK_SECONDS).round() as usize).saturating_sub(1)
}
