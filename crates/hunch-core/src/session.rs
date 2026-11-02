use std::mem;

use crate::segment::{SegmentEvent, Segmenter};
use crate::{Analyzer, Estimate};

/// Windows shorter than this are UI sounds and notifications rather than music.
const MIN_WINDOW_SECONDS: f32 = 4.0;

/// Times are seconds of audio pushed since the session was created.
#[derive(Debug, Clone, PartialEq)]
pub enum SessionEvent {
    WindowStarted {
        window_id: u32,
        at_seconds: f32,
    },
    /// Sent about twice per second of window audio.
    Estimate {
        window_id: u32,
        estimate: Estimate,
    },
    WindowEnded {
        window_id: u32,
        /// The final estimate, or `None` if the window never produced one.
        estimate: Option<Estimate>,
        duration_seconds: f32,
        too_short: bool,
    },
}

struct OpenWindow {
    id: u32,
    start_sample: u64,
    has_estimate: bool,
}

/// Streaming listening session: finds windows of sound and analyzes each one from scratch.
///
/// Audio the segmenter hasn't settled yet is held back, so a window's trailing silence is never
/// analyzed and a short gap inside a window is analyzed once the sound resumes.
pub struct Session {
    sample_rate: u32,
    segmenter: Segmenter,
    analyzer: Analyzer,
    /// Samples from `pending_start` on that are neither analyzed nor discarded yet.
    pending: Vec<f32>,
    pending_start: u64,
    segment_events: Vec<SegmentEvent>,
    window: Option<OpenWindow>,
    next_window_id: u32,
    samples_pushed: u64,
    /// `samples_pushed` at the last reset, where the segmenter's sample positions start.
    clock_offset: u64,
}

impl Session {
    pub fn new(sample_rate: u32) -> Self {
        let sample_rate = sample_rate.max(1);
        Self {
            sample_rate,
            segmenter: Segmenter::new(sample_rate),
            analyzer: Analyzer::new(sample_rate),
            pending: Vec::new(),
            pending_start: 0,
            segment_events: Vec::new(),
            window: None,
            next_window_id: 1,
            samples_pushed: 0,
            clock_offset: 0,
        }
    }

    /// Mono f32 samples in [-1, 1]. Returns what happened, in order; usually nothing.
    pub fn push(&mut self, samples: &[f32]) -> Vec<SessionEvent> {
        let mut events = Vec::new();
        self.pending.extend_from_slice(samples);
        self.samples_pushed += samples.len() as u64;

        let mut segment_events = mem::take(&mut self.segment_events);
        self.segmenter
            .push(samples, |event| segment_events.push(event));
        for event in segment_events.drain(..) {
            match event {
                SegmentEvent::Started { at_sample } => self.start_window(at_sample, &mut events),
                SegmentEvent::Ended { at_sample } => self.end_window(at_sample, &mut events),
            }
        }
        self.segment_events = segment_events;

        let undecided_from = self.segmenter.undecided_from();
        if self.window.is_some() {
            self.analyze_until(undecided_from, &mut events);
        } else {
            self.discard_until(undecided_from);
        }
        events
    }

    /// Drops any open window without ending it. Window ids and times keep counting.
    pub fn reset(&mut self) {
        self.clock_offset = self.samples_pushed;
        self.segmenter.reset();
        self.analyzer.reset();
        self.pending.clear();
        self.pending_start = 0;
        self.window = None;
    }

    fn start_window(&mut self, at_sample: u64, events: &mut Vec<SessionEvent>) {
        self.discard_until(at_sample);
        self.analyzer.reset();
        let window_id = self.next_window_id;
        self.next_window_id += 1;
        self.window = Some(OpenWindow {
            id: window_id,
            start_sample: at_sample,
            has_estimate: false,
        });
        events.push(SessionEvent::WindowStarted {
            window_id,
            at_seconds: self.seconds_since_creation(at_sample),
        });
    }

    fn end_window(&mut self, at_sample: u64, events: &mut Vec<SessionEvent>) {
        self.analyze_until(at_sample, events);
        let Some(window) = self.window.take() else {
            return;
        };
        let duration_seconds = self.seconds(at_sample - window.start_sample);
        events.push(SessionEvent::WindowEnded {
            window_id: window.id,
            estimate: window.has_estimate.then(|| self.analyzer.current()),
            duration_seconds,
            too_short: duration_seconds < MIN_WINDOW_SECONDS,
        });
    }

    /// Feeds pending audio before `end_sample` to the analyzer, split at the analyzer's estimate
    /// cadence so estimates land at the same window times however the input was chunked.
    fn analyze_until(&mut self, end_sample: u64, events: &mut Vec<SessionEvent>) {
        let count = self.pending_count_until(end_sample);
        let Some(window) = self.window.as_mut() else {
            return;
        };

        let mut fed = 0;
        while fed < count {
            let until_estimate =
                usize::try_from(self.analyzer.samples_until_estimate()).unwrap_or(usize::MAX);
            let step = (count - fed).min(until_estimate);
            if let Some(estimate) = self.analyzer.push(&self.pending[fed..fed + step]) {
                window.has_estimate = true;
                events.push(SessionEvent::Estimate {
                    window_id: window.id,
                    estimate,
                });
            }
            fed += step;
        }
        self.drop_pending(count);
    }

    fn discard_until(&mut self, end_sample: u64) {
        let count = self.pending_count_until(end_sample);
        self.drop_pending(count);
    }

    fn pending_count_until(&self, end_sample: u64) -> usize {
        let count = end_sample.saturating_sub(self.pending_start);
        usize::try_from(count)
            .unwrap_or(usize::MAX)
            .min(self.pending.len())
    }

    fn drop_pending(&mut self, count: usize) {
        self.pending.drain(..count);
        self.pending_start += count as u64;
    }

    fn seconds_since_creation(&self, sample: u64) -> f32 {
        self.seconds(self.clock_offset + sample)
    }

    fn seconds(&self, samples: u64) -> f32 {
        (samples as f64 / f64::from(self.sample_rate)) as f32
    }
}

#[cfg(test)]
mod tests;
