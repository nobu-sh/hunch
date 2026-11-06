use std::collections::VecDeque;
use std::mem;

use crate::change::MAX_CHANGE_AGE_SECONDS;
use crate::segment::{SegmentEvent, Segmenter};
use crate::{Analyzer, ChangeDetector, Estimate, Observation, TempoChange};

/// Windows shorter than this are UI sounds and notifications rather than music.
const MIN_WINDOW_SECONDS: f32 = 4.0;
/// Analyzed window audio kept for re-analysis after a split: the oldest change point the detector
/// can report, plus a second for its search grid's slack. About 2.2 MB at 22,050 Hz.
const HISTORY_SECONDS: f32 = MAX_CHANGE_AGE_SECONDS + 1.0;

/// Why a window started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowStartReason {
    /// Sound began after silence.
    Sound,
    /// The previous window's song changed into this one without a pause.
    Change,
}

/// Why a window ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowEndReason {
    /// The sound stopped.
    Silence,
    /// The tempo changed: a new song began without a pause, and a new window starts where this one
    /// ends.
    Change,
    /// `Session::finish`: the stream stopped.
    Finished,
}

/// Times are seconds of audio pushed since the session was created.
///
/// A window ended by a change is followed right away by the next one, which starts where it ended
/// (`at_seconds` of the new window is the old one's start plus its `duration_seconds`). The change
/// is found several seconds after it happened, so the old window has already sent estimates for
/// audio past its end, and the new window's estimates for the audio since the change all arrive at
/// once.
#[derive(Debug, Clone, PartialEq)]
pub enum SessionEvent {
    WindowStarted {
        window_id: u32,
        at_seconds: f32,
        reason: WindowStartReason,
    },
    /// Sent once per half second of window audio.
    Estimate { window_id: u32, estimate: Estimate },
    WindowEnded {
        window_id: u32,
        /// The final estimate, or `None` if the window never produced one. For a change, the
        /// estimate at the change point.
        estimate: Option<Estimate>,
        duration_seconds: f32,
        too_short: bool,
        reason: WindowEndReason,
    },
}

/// An estimate and the sample position (segmenter clock) of the audio it covers up to.
struct TimedEstimate {
    end_sample: u64,
    estimate: Estimate,
}

struct OpenWindow {
    id: u32,
    start_sample: u64,
    /// The window's estimates since the start of `Session::history`, plus the one before it.
    estimates: VecDeque<TimedEstimate>,
}

impl OpenWindow {
    fn new(id: u32, start_sample: u64) -> Self {
        Self {
            id,
            start_sample,
            estimates: VecDeque::new(),
        }
    }

    fn estimate_at(&self, sample: u64) -> Option<Estimate> {
        self.estimates
            .iter()
            .rev()
            .find(|timed| timed.end_sample <= sample)
            .map(|timed| timed.estimate.clone())
    }

    /// Drops estimates no change point can reach back to, keeping the last one at or before
    /// `oldest_sample`.
    fn forget_before(&mut self, oldest_sample: u64) {
        while self
            .estimates
            .get(1)
            .is_some_and(|next| next.end_sample <= oldest_sample)
        {
            self.estimates.pop_front();
        }
    }
}

/// Streaming listening session: finds windows of sound and analyzes each one from scratch.
///
/// Audio the segmenter hasn't settled yet is held back, so a window's trailing silence is never
/// analyzed and a short gap inside a window is analyzed once the sound resumes.
///
/// Inside a window, a `ChangeDetector` watches for a new song's tempo. When it finds one, the window
/// ends at the change point and a new window starts there, analyzed from the kept audio since then.
pub struct Session {
    sample_rate: u32,
    segmenter: Segmenter,
    analyzer: Analyzer,
    detector: ChangeDetector,
    /// Samples from `pending_start` on that are neither analyzed nor discarded yet.
    pending: Vec<f32>,
    pending_start: u64,
    /// The open window's settled audio from `history_start` on, kept for re-analysis after a split.
    history: Vec<f32>,
    history_start: u64,
    /// The next sample of `history` the analyzer reads.
    analyzed_until: u64,
    history_limit: u64,
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
            detector: ChangeDetector::new(),
            pending: Vec::new(),
            pending_start: 0,
            history: Vec::new(),
            history_start: 0,
            analyzed_until: 0,
            history_limit: (HISTORY_SECONDS * sample_rate as f32).ceil() as u64,
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
                SegmentEvent::Ended { at_sample } => self.end_window(
                    WindowEnd {
                        at_sample,
                        reason: WindowEndReason::Silence,
                    },
                    &mut events,
                ),
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

    /// For a stream that has stopped: ends any open window as if its sound stopped here, then
    /// resets. The ending events (any last estimates, then `WindowEnded` with reason `Finished`)
    /// follow the same rules as a window that silence closed, including `too_short`. A quiet
    /// stretch at the end that hadn't closed the window yet is left out of it.
    pub fn finish(&mut self) -> Vec<SessionEvent> {
        let mut events = Vec::new();
        if let Some(at_sample) = self.segmenter.open_window_end() {
            let end = WindowEnd {
                at_sample,
                reason: WindowEndReason::Finished,
            };
            self.end_window(end, &mut events);
        }
        self.reset();
        events
    }

    /// Drops any open window without ending it. Window ids and times keep counting.
    pub fn reset(&mut self) {
        self.clock_offset = self.samples_pushed;
        self.segmenter.reset();
        self.analyzer.reset();
        self.detector.reset();
        self.pending.clear();
        self.pending_start = 0;
        self.restart_history(0);
        self.window = None;
    }

    fn start_window(&mut self, at_sample: u64, events: &mut Vec<SessionEvent>) {
        self.discard_until(at_sample);
        self.restart_history(at_sample);
        events.push(self.open_window(WindowStart {
            at_sample,
            reason: WindowStartReason::Sound,
        }));
    }

    fn end_window(&mut self, end: WindowEnd, events: &mut Vec<SessionEvent>) {
        self.analyze_until(end.at_sample, events);
        let Some(window) = self.window.take() else {
            return;
        };
        let has_estimate = !window.estimates.is_empty();
        let estimate = has_estimate.then(|| self.analyzer.current());
        events.push(self.ended_event(EndedWindow {
            window,
            estimate,
            end,
        }));
    }

    /// Closes the open window at the change point and opens the next one there. The caller goes on
    /// analyzing from `analyzed_until`, which is moved back to the change point.
    fn split(&mut self, change: TempoChange, events: &mut Vec<SessionEvent>) {
        let Some(window) = self.window.take() else {
            return;
        };
        let window_samples = (f64::from(change.at_seconds) * f64::from(self.sample_rate)).round();
        let at_sample = (window.start_sample + window_samples as u64)
            .clamp(self.history_start, self.analyzed_until);
        let estimate = window.estimate_at(at_sample);
        let end = WindowEnd {
            at_sample,
            reason: WindowEndReason::Change,
        };
        events.push(self.ended_event(EndedWindow {
            window,
            estimate,
            end,
        }));

        self.forget_history_before(at_sample);
        self.analyzed_until = at_sample;
        events.push(self.open_window(WindowStart {
            at_sample,
            reason: WindowStartReason::Change,
        }));
    }

    /// Opens a window with a fresh analyzer and detector, returning its `WindowStarted`.
    fn open_window(&mut self, start: WindowStart) -> SessionEvent {
        self.analyzer.reset();
        self.detector.reset();
        let window_id = self.next_window_id;
        self.next_window_id += 1;
        self.window = Some(OpenWindow::new(window_id, start.at_sample));
        SessionEvent::WindowStarted {
            window_id,
            at_seconds: self.seconds_since_creation(start.at_sample),
            reason: start.reason,
        }
    }

    fn ended_event(&self, ended: EndedWindow) -> SessionEvent {
        let EndedWindow {
            window,
            estimate,
            end,
        } = ended;
        let duration_seconds = self.seconds(end.at_sample - window.start_sample);
        SessionEvent::WindowEnded {
            window_id: window.id,
            estimate,
            duration_seconds,
            too_short: duration_seconds < MIN_WINDOW_SECONDS,
            reason: end.reason,
        }
    }

    /// Moves pending audio before `end_sample` into the window's history and analyzes it.
    fn analyze_until(&mut self, end_sample: u64, events: &mut Vec<SessionEvent>) {
        if self.window.is_none() {
            return;
        }
        let count = self.pending_count_until(end_sample);
        self.history.extend_from_slice(&self.pending[..count]);
        self.drop_pending(count);
        self.analyze_history(events);
    }

    /// Feeds the history the analyzer hasn't read yet, split at the analyzer's estimate cadence so
    /// estimates land at the same window times however the input was chunked. After a split it
    /// reads on from the change point, so the new window catches up on the audio since then.
    fn analyze_history(&mut self, events: &mut Vec<SessionEvent>) {
        let history_end = self.history_start + self.history.len() as u64;
        while self.analyzed_until < history_end {
            let Some(window) = self.window.as_mut() else {
                return;
            };
            let step =
                (history_end - self.analyzed_until).min(self.analyzer.samples_until_estimate());
            let from = (self.analyzed_until - self.history_start) as usize;
            let chunk = &self.history[from..from + step as usize];
            let estimate = self.analyzer.push(chunk);
            self.analyzed_until += step;
            let Some(estimate) = estimate else {
                continue;
            };

            let change = self.detector.update(Observation {
                analyzer: &self.analyzer,
                estimate: &estimate,
            });
            window.estimates.push_back(TimedEstimate {
                end_sample: self.analyzed_until,
                estimate: estimate.clone(),
            });
            events.push(SessionEvent::Estimate {
                window_id: window.id,
                estimate,
            });
            if let Some(change) = change {
                self.split(change, events);
            }
        }
        let oldest_kept = self.analyzed_until.saturating_sub(self.history_limit);
        self.forget_history_before(oldest_kept);
    }

    /// Empties the history for a window that starts at `start_sample`.
    fn restart_history(&mut self, start_sample: u64) {
        self.history.clear();
        self.history_start = start_sample;
        self.analyzed_until = start_sample;
    }

    fn forget_history_before(&mut self, sample: u64) {
        let count = sample.saturating_sub(self.history_start) as usize;
        let count = count.min(self.history.len());
        self.history.drain(..count);
        self.history_start += count as u64;
        if let Some(window) = self.window.as_mut() {
            window.forget_before(self.history_start);
        }
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

struct WindowStart {
    at_sample: u64,
    reason: WindowStartReason,
}

struct WindowEnd {
    at_sample: u64,
    reason: WindowEndReason,
}

struct EndedWindow {
    window: OpenWindow,
    estimate: Option<Estimate>,
    end: WindowEnd,
}

#[cfg(test)]
mod tests;
