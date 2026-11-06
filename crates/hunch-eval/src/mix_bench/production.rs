//! The production change detector, run through the production `hunch_core::Session`: its splits are
//! the windows it ends on a tempo change. It can't replay a trace, because a split restarts the
//! analyzer, so it runs on the clip's audio.

use hunch_core::{Session, SessionEvent, WindowEndReason};

use super::detect::{Cause, Split};
use super::trace::TICK_SECONDS;
use crate::resample::ANALYSIS_SAMPLE_RATE;
use crate::run;

/// The bench's name for this detector.
pub const CONFIG: &str = "core";

pub fn splits(samples: &[f32]) -> Vec<Split> {
    let rate = ANALYSIS_SAMPLE_RATE as f32;
    let mut session = Session::new(ANALYSIS_SAMPLE_RATE);
    let mut splits = Vec::new();
    let mut window_start_seconds = 0.0;
    let mut pushed = 0;
    for chunk in samples.chunks(run::chunk_len()) {
        pushed += chunk.len();
        for event in session.push(chunk) {
            match event {
                SessionEvent::WindowStarted { at_seconds, .. } => {
                    window_start_seconds = at_seconds;
                }
                SessionEvent::WindowEnded {
                    duration_seconds,
                    reason: WindowEndReason::Change,
                    ..
                } => splits.push(Split {
                    detect_tick: tick_at(pushed as f32 / rate),
                    change_tick: tick_at(window_start_seconds + duration_seconds),
                    cause: Cause::Tempo,
                }),
                SessionEvent::WindowEnded { .. } | SessionEvent::Estimate { .. } => {}
            }
        }
    }
    splits
}

/// The trace tick whose estimate is at `seconds` (the inverse of `Trace::seconds_of`).
fn tick_at(seconds: f32) -> usize {
    ((seconds / TICK_SECONDS).round() as usize).saturating_sub(1)
}
