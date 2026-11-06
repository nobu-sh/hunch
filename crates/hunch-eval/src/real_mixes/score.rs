//! Scores a session's events on a recording against its hand marks.
//!
//! Only window boundaries and estimates are looked at, never why a window ended, so any split
//! rule the session gains is scored the same way.

use hunch_core::{Estimate, SessionEvent};

use crate::metrics;
use crate::real_mixes::marks::Mark;

/// A boundary detected this long after a change mark still counts as finding it.
pub const MATCH_AFTER_SECONDS: f32 = 30.0;
/// Hand marks are rough, so a boundary detected up to this long before a mark also counts.
pub const MATCH_BEFORE_SECONDS: f32 = 5.0;
/// Readouts are checked this far into each segment, and at its end.
pub const READOUT_OFFSETS_SECONDS: [f32; 2] = [10.0, 30.0];
pub const READOUT_CHECKPOINTS: usize = READOUT_OFFSETS_SECONDS.len() + 1;
const SECONDS_PER_HOUR: f32 = 3600.0;

/// A session event and how much audio had been pushed when it came out.
#[derive(Debug, Clone, PartialEq)]
pub struct TimedEvent {
    pub emitted_at: f32,
    pub event: SessionEvent,
}

/// Where one window gave way to the next.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Boundary {
    /// When the session said so: the old window's `WindowEnded`.
    pub detected_at: f32,
    /// The next window's `at_seconds`.
    pub change_point: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkKind {
    /// The first row: nothing before it to change from.
    First,
    Change {
        key: bool,
        bpm: bool,
    },
    /// Neither the key nor the BPM changes (a drop, a switch-up, or an unmarked side).
    Unchanged,
}

/// What became of one mark after the first.
#[derive(Debug, Clone, PartialEq)]
pub struct MarkOutcome {
    pub kind: MarkKind,
    pub start_seconds: f32,
    pub note: String,
    /// The boundary matched to it, if one landed close enough.
    pub boundary: Option<Boundary>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Tally {
    pub correct: usize,
    pub scored: usize,
}

impl Tally {
    fn add(&mut self, is_correct: bool) {
        self.scored += 1;
        self.correct += usize::from(is_correct);
    }

    fn merge(&mut self, other: Tally) {
        self.correct += other.correct;
        self.scored += other.scored;
    }

    pub fn accuracy(self) -> Option<f64> {
        metrics::ratio(self.correct, self.scored)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Score {
    /// Audio from the first mark to the end.
    pub scored_seconds: f32,
    pub changes: usize,
    /// Seconds from each found change mark to its boundary's detection.
    pub delays: Vec<f32>,
    /// Each found change's boundary change point minus its mark.
    pub change_point_errors: Vec<f32>,
    pub unchanged_marks: usize,
    /// Unchanged marks with a boundary close by; those boundaries are false splits too.
    pub unchanged_marks_split: usize,
    pub false_splits: usize,
    /// Per checkpoint: +10 s, +30 s, end of segment.
    pub key_readouts: [Tally; READOUT_CHECKPOINTS],
    pub bpm_readouts: [Tally; READOUT_CHECKPOINTS],
}

impl Score {
    pub fn found(&self) -> usize {
        self.delays.len()
    }

    pub fn false_splits_per_hour(&self) -> Option<f64> {
        let hours = f64::from(self.scored_seconds / SECONDS_PER_HOUR);
        (hours > 0.0).then(|| self.false_splits as f64 / hours)
    }

    pub fn merge(&mut self, other: &Score) {
        self.scored_seconds += other.scored_seconds;
        self.changes += other.changes;
        self.delays.extend(&other.delays);
        self.change_point_errors.extend(&other.change_point_errors);
        self.unchanged_marks += other.unchanged_marks;
        self.unchanged_marks_split += other.unchanged_marks_split;
        self.false_splits += other.false_splits;
        for (mine, theirs) in self.key_readouts.iter_mut().zip(other.key_readouts) {
            mine.merge(theirs);
        }
        for (mine, theirs) in self.bpm_readouts.iter_mut().zip(other.bpm_readouts) {
            mine.merge(theirs);
        }
    }
}

pub struct ScoreInput<'a> {
    pub marks: &'a [Mark],
    pub events: &'a [TimedEvent],
    pub audio_seconds: f32,
}

pub fn score(input: &ScoreInput<'_>) -> (Score, Vec<MarkOutcome>) {
    let Some(first) = input.marks.first() else {
        return (Score::default(), Vec::new());
    };
    let boundaries = boundaries(input.events);
    let outcomes = match_marks(input.marks, &boundaries);
    let matched_to_changes = outcomes
        .iter()
        .filter(|outcome| matches!(outcome.kind, MarkKind::Change { .. }))
        .filter_map(|outcome| outcome.boundary)
        .count();
    let boundaries_in_scored_audio = boundaries
        .iter()
        .filter(|boundary| boundary.detected_at >= first.start_seconds)
        .count();

    let mut score = Score {
        scored_seconds: (input.audio_seconds - first.start_seconds).max(0.0),
        false_splits: boundaries_in_scored_audio.saturating_sub(matched_to_changes),
        ..Score::default()
    };
    for outcome in &outcomes {
        match outcome.kind {
            MarkKind::First => {}
            MarkKind::Change { .. } => {
                score.changes += 1;
                if let Some(boundary) = outcome.boundary {
                    score
                        .delays
                        .push(boundary.detected_at - outcome.start_seconds);
                    score
                        .change_point_errors
                        .push(boundary.change_point - outcome.start_seconds);
                }
            }
            MarkKind::Unchanged => {
                score.unchanged_marks += 1;
                score.unchanged_marks_split += usize::from(outcome.boundary.is_some());
            }
        }
    }
    score_readouts(&mut score, input);
    (score, outcomes)
}

pub fn classify(previous: &Mark, mark: &Mark) -> MarkKind {
    let is_key_change = !previous.keys.is_empty()
        && !mark.keys.is_empty()
        && !previous
            .keys
            .iter()
            .any(|&key| metrics::is_same_scale_as_any(key, &mark.keys));
    let is_bpm_change = match (previous.bpm, mark.bpm) {
        (Some(before), Some(after)) => !metrics::is_tempo_match(after, before),
        _ => false,
    };
    if is_key_change || is_bpm_change {
        MarkKind::Change {
            key: is_key_change,
            bpm: is_bpm_change,
        }
    } else {
        MarkKind::Unchanged
    }
}

/// Every place a window gave way to a later one. The first window's start and the last window's
/// end aren't boundaries.
pub fn boundaries(events: &[TimedEvent]) -> Vec<Boundary> {
    let mut boundaries = Vec::new();
    let mut has_started = false;
    let mut ended_at = None;
    for timed in events {
        match timed.event {
            SessionEvent::WindowEnded { .. } => ended_at = Some(timed.emitted_at),
            SessionEvent::WindowStarted { at_seconds, .. } => {
                if has_started {
                    boundaries.push(Boundary {
                        detected_at: ended_at.unwrap_or(timed.emitted_at),
                        change_point: at_seconds,
                    });
                }
                has_started = true;
                ended_at = None;
            }
            SessionEvent::Estimate { .. } => {}
        }
    }
    boundaries
}

/// Gives each mark after the first the earliest unused boundary detected close to it, change
/// marks first so an unchanged mark nearby can't take a change's boundary.
fn match_marks(marks: &[Mark], boundaries: &[Boundary]) -> Vec<MarkOutcome> {
    let mut outcomes: Vec<MarkOutcome> = marks
        .iter()
        .enumerate()
        .map(|(index, mark)| MarkOutcome {
            kind: match index {
                0 => MarkKind::First,
                _ => classify(&marks[index - 1], mark),
            },
            start_seconds: mark.start_seconds,
            note: mark.note.clone(),
            boundary: None,
        })
        .collect();
    let mut is_used = vec![false; boundaries.len()];
    let is_change = |outcome: &MarkOutcome| matches!(outcome.kind, MarkKind::Change { .. });
    for wants_change in [true, false] {
        for outcome in outcomes.iter_mut() {
            let is_candidate =
                outcome.kind != MarkKind::First && is_change(outcome) == wants_change;
            if !is_candidate {
                continue;
            }
            let earliest = outcome.start_seconds - MATCH_BEFORE_SECONDS;
            let latest = outcome.start_seconds + MATCH_AFTER_SECONDS;
            let found = boundaries.iter().enumerate().find(|(index, boundary)| {
                !is_used[*index] && (earliest..=latest).contains(&boundary.detected_at)
            });
            if let Some((index, &boundary)) = found {
                is_used[index] = true;
                outcome.boundary = Some(boundary);
            }
        }
    }
    outcomes
}

/// What the session shows at each moment: the latest estimate of the current window, kept after
/// the window ends and cleared when the next one starts.
fn readout_at(events: &[TimedEvent], seconds: f32) -> Option<&Estimate> {
    let mut readout = None;
    for timed in events
        .iter()
        .take_while(|timed| timed.emitted_at <= seconds)
    {
        match &timed.event {
            SessionEvent::WindowStarted { .. } => readout = None,
            SessionEvent::Estimate { estimate, .. } => readout = Some(estimate),
            SessionEvent::WindowEnded { estimate, .. } => readout = estimate.as_ref(),
        }
    }
    readout
}

fn score_readouts(score: &mut Score, input: &ScoreInput<'_>) {
    let ends = input
        .marks
        .iter()
        .skip(1)
        .map(|mark| mark.start_seconds)
        .chain([input.audio_seconds]);
    for (mark, end) in input.marks.iter().zip(ends) {
        let offsets = READOUT_OFFSETS_SECONDS.map(|offset| mark.start_seconds + offset);
        let checkpoints = offsets.into_iter().map(Some).chain([Some(end)]);
        for (checkpoint, seconds) in checkpoints.enumerate() {
            let Some(seconds) = seconds.filter(|&seconds| seconds <= end) else {
                continue;
            };
            let readout = readout_at(input.events, seconds);
            if !mark.keys.is_empty() {
                let key = readout.and_then(|estimate| estimate.key);
                let is_correct =
                    key.is_some_and(|key| metrics::is_same_scale_as_any(key, &mark.keys));
                score.key_readouts[checkpoint].add(is_correct);
            }
            if let Some(bpm) = mark.bpm {
                let shown = readout.and_then(|estimate| estimate.bpm);
                let is_correct = shown.is_some_and(|shown| metrics::is_tempo_match(shown, bpm));
                score.bpm_readouts[checkpoint].add(is_correct);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use hunch_core::{Key, Mode, PitchClass};

    use super::*;

    const A: u8 = 9;
    const D: u8 = 2;

    fn key(index: u8, mode: Mode) -> Key {
        let tonic = PitchClass::new(index).expect("valid pitch class");
        Key { tonic, mode }
    }

    fn mark(start_seconds: f32, keys: &[Key], bpm: Option<f32>) -> Mark {
        Mark {
            start_seconds,
            keys: keys.to_vec(),
            bpm,
            note: String::new(),
        }
    }

    fn estimate(key: Key, bpm: f32) -> Estimate {
        Estimate {
            key: Some(key),
            key_confidence: 0.8,
            bpm: Some(bpm),
            bpm_confidence: 0.8,
            bpm_alternate: None,
            tuning_cents: None,
            seconds_analyzed: 10.0,
        }
    }

    fn started(emitted_at: f32, window_id: u32, at_seconds: f32) -> TimedEvent {
        TimedEvent {
            emitted_at,
            event: SessionEvent::WindowStarted {
                window_id,
                at_seconds,
            },
        }
    }

    fn ended(emitted_at: f32, window_id: u32, last: Option<Estimate>) -> TimedEvent {
        TimedEvent {
            emitted_at,
            event: SessionEvent::WindowEnded {
                window_id,
                estimate: last,
                duration_seconds: 0.0,
                too_short: false,
            },
        }
    }

    fn shows(emitted_at: f32, window_id: u32, estimate: Estimate) -> TimedEvent {
        TimedEvent {
            emitted_at,
            event: SessionEvent::Estimate {
                window_id,
                estimate,
            },
        }
    }

    #[test]
    fn a_change_is_a_new_scale_or_a_tempo_off_by_more_than_four_percent() {
        let a_minor = key(A, Mode::Minor);
        let c_major = key(0, Mode::Major);
        let d_minor = key(D, Mode::Minor);
        let base = mark(0.0, &[a_minor], Some(124.0));

        assert_eq!(
            classify(&base, &mark(1.0, &[c_major], Some(126.0))),
            MarkKind::Unchanged
        );
        assert_eq!(
            classify(&base, &mark(1.0, &[d_minor], Some(124.0))),
            MarkKind::Change {
                key: true,
                bpm: false
            }
        );
        assert_eq!(
            classify(&base, &mark(1.0, &[a_minor], Some(132.0))),
            MarkKind::Change {
                key: false,
                bpm: true
            }
        );
    }

    #[test]
    fn unmarked_sides_never_change() {
        let base = mark(0.0, &[key(A, Mode::Minor)], None);
        let next = mark(1.0, &[], Some(140.0));
        assert_eq!(classify(&base, &next), MarkKind::Unchanged);
    }

    #[test]
    fn a_split_is_detected_at_the_end_and_placed_at_the_next_start() {
        let events = [
            started(0.5, 1, 0.0),
            ended(70.0, 1, None),
            started(70.0, 2, 61.0),
            ended(100.0, 2, None),
            started(103.0, 3, 102.0),
            ended(120.0, 3, None),
        ];
        assert_eq!(
            boundaries(&events),
            vec![
                Boundary {
                    detected_at: 70.0,
                    change_point: 61.0
                },
                Boundary {
                    detected_at: 100.0,
                    change_point: 102.0
                },
            ]
        );
    }

    #[test]
    fn scores_found_and_missed_changes_and_false_splits() {
        let a_minor = key(A, Mode::Minor);
        let d_minor = key(D, Mode::Minor);
        let marks = [
            mark(0.0, &[a_minor], Some(124.0)),
            mark(60.0, &[d_minor], Some(124.0)),
            mark(120.0, &[d_minor], Some(124.0)),
            mark(180.0, &[a_minor], Some(124.0)),
        ];
        let events = [
            started(0.5, 1, 0.0),
            // A false split mid-segment.
            ended(30.0, 1, None),
            started(30.0, 2, 29.0),
            // Finds the change at 60 s, 8 s late, placing it 1 s early.
            ended(68.0, 2, None),
            started(68.0, 3, 59.0),
            // Splits on the unchanged mark at 120 s.
            ended(121.0, 3, None),
            started(121.0, 4, 120.0),
            // Misses the change at 180 s.
            ended(240.0, 4, None),
        ];
        let (score, outcomes) = score(&ScoreInput {
            marks: &marks,
            events: &events,
            audio_seconds: 240.0,
        });

        assert_eq!(score.changes, 2);
        assert_eq!(score.found(), 1);
        assert_eq!(score.delays, vec![8.0]);
        assert_eq!(score.change_point_errors, vec![-1.0]);
        assert_eq!(score.unchanged_marks, 1);
        assert_eq!(score.unchanged_marks_split, 1);
        assert_eq!(score.false_splits, 2);
        let per_hour = score.false_splits_per_hour().expect("scored audio");
        assert!((per_hour - 30.0).abs() < 1e-3, "{per_hour}");
        assert_eq!(outcomes[3].boundary, None);
    }

    #[test]
    fn a_boundary_just_before_a_mark_still_finds_it() {
        let marks = [mark(0.0, &[], Some(124.0)), mark(60.0, &[], Some(140.0))];
        let events = [
            started(0.5, 1, 0.0),
            ended(57.0, 1, None),
            started(58.0, 2, 58.0),
        ];
        let (score, _) = score(&ScoreInput {
            marks: &marks,
            events: &events,
            audio_seconds: 120.0,
        });
        assert_eq!(score.delays, vec![-3.0]);
        assert_eq!(score.false_splits, 0);
    }

    #[test]
    fn readouts_are_checked_at_ten_and_thirty_seconds_and_at_the_end() {
        let a_minor = key(A, Mode::Minor);
        let d_minor = key(D, Mode::Minor);
        let marks = [
            mark(0.0, &[a_minor], Some(124.0)),
            mark(40.0, &[d_minor], Some(140.0)),
        ];
        let events = [
            started(0.5, 1, 0.0),
            shows(5.0, 1, estimate(a_minor, 124.0)),
            // Wrong tempo by 30 s, then right again before the end.
            shows(25.0, 1, estimate(a_minor, 100.0)),
            shows(35.0, 1, estimate(a_minor, 124.0)),
            // The old readout lingers into the next segment, so +10 s is wrong.
            ended(55.0, 1, Some(estimate(a_minor, 124.0))),
            started(55.0, 2, 41.0),
            shows(60.0, 2, estimate(d_minor, 140.0)),
        ];
        let (score, _) = score(&ScoreInput {
            marks: &marks,
            events: &events,
            audio_seconds: 65.0,
        });

        // Segment 1 checks 10, 30 and 40 s; segment 2 checks 50 and 65 s (70 s is past its end).
        assert_eq!(
            score.key_readouts,
            [
                Tally {
                    correct: 1,
                    scored: 2
                },
                Tally {
                    correct: 1,
                    scored: 1
                },
                Tally {
                    correct: 2,
                    scored: 2
                },
            ]
        );
        assert_eq!(
            score.bpm_readouts,
            [
                Tally {
                    correct: 1,
                    scored: 2
                },
                Tally {
                    correct: 0,
                    scored: 1
                },
                Tally {
                    correct: 2,
                    scored: 2
                },
            ]
        );
    }

    #[test]
    fn a_new_window_without_an_estimate_shows_nothing() {
        let a_minor = key(A, Mode::Minor);
        let events = [
            started(0.5, 1, 0.0),
            shows(5.0, 1, estimate(a_minor, 124.0)),
            ended(20.0, 1, Some(estimate(a_minor, 124.0))),
            started(20.0, 2, 20.0),
        ];
        assert!(readout_at(&events, 19.0).is_some());
        assert!(readout_at(&events, 21.0).is_none());
    }

    #[test]
    fn merging_adds_up_files() {
        let mut total = Score::default();
        let one = Score {
            scored_seconds: 1800.0,
            changes: 2,
            delays: vec![5.0],
            false_splits: 1,
            ..Score::default()
        };
        total.merge(&one);
        total.merge(&one);
        assert_eq!(total.changes, 4);
        assert_eq!(total.found(), 2);
        assert_eq!(total.false_splits_per_hour(), Some(2.0));
    }
}
