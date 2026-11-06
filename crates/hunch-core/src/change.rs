//! Track-change detection inside a listening window, from tempo.
//!
//! Silence ends a window, but mixes and crossfaded playlists go straight into the next song. After
//! each estimate the detector compares the beat of the last few seconds with the window's beat from
//! before them; when they confidently disagree for long enough, it finds where the new beat took
//! over so the caller can close the window there.
//!
//! The values come from `hunch-eval mix-bench` on Beatport val (config `core`, the production
//! version of the bench's `grid tempo6s@0.6 nophase n4s`).

use std::collections::VecDeque;

use crate::analyzer::ESTIMATE_INTERVAL_SECONDS;
use crate::{Analyzer, Estimate};

/// Onset history the recent tempo is estimated on. 4 s is too noisy to be confident; 8 and 12 s
/// detect later, and 12 s costs 33% of the analyzer instead of 11%.
const RECENT_SECONDS: f32 = 6.0;
/// The recent tempo's confidence needed to count as a new beat. 0.8 halves recall (23% vs 40% of
/// changes); 0.5 doubles false splits (0.86/h vs 0.43/h of single tracks).
const MIN_NEW_CONFIDENCE: f32 = 0.6;
/// The window's tempo before the recent stretch must be at least this confident, so a beat that
/// was never settled can't be "replaced".
const MIN_OLD_CONFIDENCE: f32 = 0.5;
/// Tempi within this fraction of each other, or of half or double, are the same beat: half-time
/// breakdowns and the tempo detector's octave flips stay in one window.
const SAME_BEAT_TOLERANCE: f32 = 0.04;
const SAME_BEAT_RATIOS: [f32; 3] = [1.0, 2.0, 0.5];
/// How long the beats must disagree before a split. 2 s makes 1.40 false splits/h; 6 s drops
/// recall from 40% to 31% of changes.
const SUSTAIN_SECONDS: f32 = 4.0;
/// Window audio the old tempo must be estimated from: a few bars, so it's the song's beat rather
/// than an intro.
const MIN_OLD_SECONDS: f32 = 8.0;
/// The change point is searched at most this far before the disagreement began, which covers
/// 16 s crossfades.
const LOOKBACK_SECONDS: f32 = 20.0;
/// The earliest change point, in seconds into the window. The window's old beat came from at least
/// `MIN_OLD_SECONDS` of audio, so at least half of that was the old song. Without the floor, a
/// change point placed inside a crossfade starts a window whose estimate still reads the old beat,
/// and it splits again at its own start, over and over.
const MIN_CHANGE_SECONDS: f32 = MIN_OLD_SECONDS / 2.0;
/// Half-width of the onset stretch each change-point candidate compares beat periods on.
const CUSUM_HALF_SECONDS: f32 = 1.5;

const RECENT_TICKS: usize = ticks(RECENT_SECONDS);
const SUSTAIN_TICKS: usize = ticks(SUSTAIN_SECONDS);

/// How far before the estimate that reports a change its change point can be: the lookback before
/// the run of disagreement, plus the run.
pub(crate) const MAX_CHANGE_AGE_SECONDS: f32 =
    LOOKBACK_SECONDS + (SUSTAIN_TICKS - 1) as f32 * ESTIMATE_INTERVAL_SECONDS;

const fn ticks(seconds: f32) -> usize {
    (seconds / ESTIMATE_INTERVAL_SECONDS) as usize
}

/// A detected tempo change: where the new song's beat took over.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TempoChange {
    /// Seconds into the window (`Estimate::seconds_analyzed`'s clock), on the estimate grid.
    pub at_seconds: f32,
    /// The window's tempo before the change.
    pub old_bpm: f32,
    /// The tempo of the last few seconds.
    pub new_bpm: f32,
}

/// What the detector reads after each analyzer estimate.
#[derive(Clone, Copy)]
pub struct Observation<'a> {
    /// The window's analyzer, which just produced `estimate`.
    pub analyzer: &'a Analyzer,
    pub estimate: &'a Estimate,
}

#[derive(Debug, Clone, Copy)]
struct WindowTempo {
    bpm: Option<f32>,
    confidence: f32,
    seconds: f32,
}

/// Watches one listening window for a confident change of beat. Feed it after every estimate of
/// the window's analyzer (`update`), and `reset` it when a new window starts.
///
/// A split needs the last 6 s of onsets to show a confident beat (≥ 0.6) that isn't the window's
/// earlier beat (≥ 0.5 confident, from at least 8 s of audio) at 1×, 2× or ½× within 4%, for 4 s
/// in a row, and a point in the lookback where the onsets start fitting the new beat better.
pub struct ChangeDetector {
    /// The window's tempo at each of the last `RECENT_TICKS + 1` estimates, oldest first: the
    /// front is the window's beat from just before the recent stretch.
    window_tempi: VecDeque<WindowTempo>,
    /// Estimates in a row the beats have disagreed.
    run: usize,
    /// The old beat when the current run of disagreement began.
    run_old_bpm: f32,
    /// Reused buffer for the onset envelope the change point is searched on.
    onsets: Vec<f32>,
}

impl Default for ChangeDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl ChangeDetector {
    pub fn new() -> Self {
        Self {
            window_tempi: VecDeque::with_capacity(RECENT_TICKS + 1),
            run: 0,
            run_old_bpm: 0.0,
            onsets: Vec::new(),
        }
    }

    /// Call after each estimate of the window's analyzer. Returns a change once the beat has
    /// confidently changed; the caller should then end the window at `at_seconds`. The detector
    /// resets itself after a change.
    pub fn update(&mut self, observation: Observation) -> Option<TempoChange> {
        let Observation { analyzer, estimate } = observation;
        if self.window_tempi.len() == RECENT_TICKS + 1 {
            self.window_tempi.pop_front();
        }
        self.window_tempi.push_back(WindowTempo {
            bpm: estimate.bpm,
            confidence: estimate.bpm_confidence,
            seconds: estimate.seconds_analyzed,
        });

        // Checked before the recent estimate, which is the expensive part.
        let Some(old_bpm) = self.settled_old_bpm() else {
            self.run = 0;
            return None;
        };
        let recent = analyzer.tempo_detector().estimate_recent(RECENT_SECONDS);
        let new_bpm = recent
            .bpm
            .filter(|&bpm| recent.confidence >= MIN_NEW_CONFIDENCE && !is_same_beat(old_bpm, bpm));
        let Some(new_bpm) = new_bpm else {
            self.run = 0;
            return None;
        };

        if self.run == 0 {
            self.run_old_bpm = old_bpm;
        }
        self.run += 1;
        if self.run < SUSTAIN_TICKS {
            return None;
        }
        let old_bpm = self.run_old_bpm;
        let at_seconds = self.change_point(ChangePointInput {
            analyzer,
            now_seconds: estimate.seconds_analyzed,
            old_bpm,
            new_bpm,
        })?;
        self.reset();
        Some(TempoChange {
            at_seconds,
            old_bpm,
            new_bpm,
        })
    }

    /// Forget the window, for a new one.
    pub fn reset(&mut self) {
        self.window_tempi.clear();
        self.run = 0;
    }

    /// The window's beat from just before the recent stretch, when it was confident and estimated
    /// from enough audio.
    fn settled_old_bpm(&self) -> Option<f32> {
        if self.window_tempi.len() <= RECENT_TICKS {
            return None;
        }
        let old = self.window_tempi.front()?;
        let is_settled = old.seconds >= MIN_OLD_SECONDS && old.confidence >= MIN_OLD_CONFIDENCE;
        old.bpm.filter(|_| is_settled)
    }

    /// Where onsets start fitting the new beat period better than the old one: the candidate time
    /// (on the estimate grid) that maximizes the suffix sum (CUSUM) of
    /// `autocorrelation(new period) − autocorrelation(old period)` from there to now.
    ///
    /// `None` when that maximum is the earliest candidate: the new beat fits better all the way
    /// back, so the beats disagree because the window's tempo was misread (an ambiguous beat, or
    /// the old song still fading out), not because a new song began.
    fn change_point(&mut self, input: ChangePointInput) -> Option<f32> {
        let ChangePointInput {
            analyzer,
            now_seconds,
            old_bpm,
            new_bpm,
        } = input;
        let timeline = analyzer.tempo_detector().combined_onsets();
        self.onsets.clear();
        self.onsets.extend(timeline.combined());
        let frame_rate = timeline.frame_rate();
        let frame_of =
            |seconds: f32| ((seconds - timeline.start_seconds()) * frame_rate).max(0.0) as usize;
        let old_lag = 60.0 / old_bpm * frame_rate;
        let new_lag = 60.0 / new_bpm * frame_rate;
        let half = (CUSUM_HALF_SECONDS * frame_rate) as usize;

        let earliest = (now_seconds - MAX_CHANGE_AGE_SECONDS).max(MIN_CHANGE_SECONDS);
        let mut suffix = 0.0;
        let mut best_sum = f32::NEG_INFINITY;
        let mut best = now_seconds;
        let mut candidate = now_seconds;
        let mut earliest_searched = now_seconds;
        while candidate >= earliest - ESTIMATE_INTERVAL_SECONDS / 2.0 {
            earliest_searched = candidate;
            let center = frame_of(candidate);
            let from = center.saturating_sub(half).min(self.onsets.len());
            let to = (center + half).min(self.onsets.len());
            let segment = &self.onsets[from..to];
            suffix += autocorrelation(segment, new_lag) - autocorrelation(segment, old_lag);
            if suffix > best_sum {
                best_sum = suffix;
                best = candidate;
            }
            candidate -= ESTIMATE_INTERVAL_SECONDS;
        }
        let has_turning_point = best != earliest_searched;
        has_turning_point.then_some(best)
    }
}

struct ChangePointInput<'a> {
    analyzer: &'a Analyzer,
    now_seconds: f32,
    old_bpm: f32,
    new_bpm: f32,
}

fn is_same_beat(old_bpm: f32, new_bpm: f32) -> bool {
    SAME_BEAT_RATIOS
        .iter()
        .any(|ratio| (new_bpm / (old_bpm * ratio) - 1.0).abs() <= SAME_BEAT_TOLERANCE)
}

/// Normalized, mean-removed autocorrelation of `segment` at `lag` frames (rounded); 0 when the
/// segment is too short or flat.
fn autocorrelation(segment: &[f32], lag: f32) -> f32 {
    let lag = lag.round() as usize;
    if lag == 0 || segment.len() <= lag + 1 {
        return 0.0;
    }
    let mean = segment.iter().sum::<f32>() / segment.len() as f32;
    let energy: f32 = segment.iter().map(|x| (x - mean).powi(2)).sum();
    if energy <= 1e-12 {
        return 0.0;
    }
    let product: f32 = segment
        .iter()
        .zip(&segment[lag..])
        .map(|(a, b)| (a - mean) * (b - mean))
        .sum();
    product / energy
}

#[cfg(test)]
mod tests;
