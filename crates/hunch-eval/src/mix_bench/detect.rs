//! Change detectors run over a trace the way a session would: one open window at a time, a split
//! closes it at the estimated change point and opens the next one there.

use std::f32::consts::TAU;

use super::features::{self, TIMBRE_BANDS};
use super::keys::{self, Evidence, KEY_COUNT, PrefixEvidence, Scorer};
use super::trace::{RECENT_TEMPO_SECONDS, TICK_SECONDS, Tempo, Trace};

/// Tempi within this fraction of each other (or of half or double) are the same beat.
const TEMPO_MATCH_TOLERANCE: f32 = 0.04;
const TEMPO_OCTAVES: [f32; 3] = [1.0, 2.0, 0.5];
/// The old side of a tempo comparison needs at least this confidence.
const MIN_OLD_TEMPO_CONFIDENCE: f32 = 0.5;
/// Half-width of the onset stretch the tempo change point compares beat periods on.
const TEMPO_CUSUM_HALF_SECONDS: f32 = 1.5;
/// Onset history the beat phase is measured on.
const PHASE_SECONDS: f32 = 4.0;
/// Lags of the onset-pattern descriptor, log-spaced over this range.
const RHYTHM_LAGS: usize = 24;
const RHYTHM_MIN_LAG_SECONDS: f32 = 0.1;
const RHYTHM_MAX_LAG_SECONDS: f32 = 1.2;
const RHYTHM_SECONDS: f32 = 3.0;
/// Change points are searched at most this far before the disagreement that triggered the split.
const LOOKBACK_SECONDS: f32 = 20.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cause {
    Key,
    Tempo,
    Novelty,
    Phase,
}

#[derive(Debug, Clone, Copy)]
pub struct Split {
    pub detect_tick: usize,
    pub change_tick: usize,
    pub cause: Cause,
}

/// A trace plus the per-tick derived data every detector shares.
pub struct Prepared<'a> {
    pub trace: &'a Trace,
    pub evidence: PrefixEvidence,
    pub scorer: &'a Scorer,
    /// Novelty descriptors, unit-norm, one per tick.
    pub chroma: Vec<Vec<f32>>,
    pub timbre: Vec<Vec<f32>>,
    pub rhythm: Vec<Vec<f32>>,
}

impl<'a> Prepared<'a> {
    pub fn new(trace: &'a Trace, scorer: &'a Scorer) -> Self {
        let evidence = PrefixEvidence::new(
            trace
                .ticks
                .iter()
                .map(|tick| Evidence::from_block(&tick.block)),
        );
        let count = trace.ticks.len();
        let chroma = (0..count)
            .map(|tick| {
                let range = evidence.range(tick.saturating_sub(1), tick + 1);
                unit(range.treble.to_vec())
            })
            .collect();
        let timbre = trace
            .ticks
            .iter()
            .map(|tick| {
                let mean = tick.timbre.iter().sum::<f32>() / TIMBRE_BANDS as f32;
                unit(tick.timbre.iter().map(|value| value - mean).collect())
            })
            .collect();
        let rhythm = (0..count)
            .map(|tick| rhythm_descriptor(&trace.flux, tick))
            .collect();
        Self {
            trace,
            evidence,
            scorer,
            chroma,
            timbre,
            rhythm,
        }
    }

    fn len(&self) -> usize {
        self.trace.ticks.len()
    }

    fn tempo(&self, tick: usize, index: usize) -> Tempo {
        self.trace.ticks[tick].recent_tempo[index]
    }

    fn scores(&self, from: usize, to: usize, min_tonal_seconds: f32) -> Option<[f32; KEY_COUNT]> {
        let evidence = self.evidence.range(from, to);
        if evidence.tonal_seconds < min_tonal_seconds {
            return None;
        }
        self.scorer.scores(&evidence)
    }
}

fn unit(mut values: Vec<f32>) -> Vec<f32> {
    let norm = values.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm > 1e-9 {
        values.iter_mut().for_each(|v| *v /= norm);
    }
    values
}

fn flux_frame(seconds: f32) -> usize {
    (seconds.max(0.0) * features::frame_rate()) as usize
}

/// Mean-removed autocorrelation of the last `RHYTHM_SECONDS` of onsets at log-spaced lags.
fn rhythm_descriptor(flux: &[f32], tick: usize) -> Vec<f32> {
    let end = flux_frame(Trace::seconds_of(tick)).min(flux.len());
    let start = end.saturating_sub(flux_frame(RHYTHM_SECONDS));
    let segment = &flux[start..end];
    let ratio =
        (RHYTHM_MAX_LAG_SECONDS / RHYTHM_MIN_LAG_SECONDS).powf(1.0 / (RHYTHM_LAGS - 1) as f32);
    let values = (0..RHYTHM_LAGS)
        .map(|index| {
            let lag = RHYTHM_MIN_LAG_SECONDS * ratio.powi(index as i32) * features::frame_rate();
            autocorrelation(segment, lag)
        })
        .collect::<Vec<f32>>();
    let mean = values.iter().sum::<f32>() / values.len() as f32;
    unit(values.iter().map(|v| v - mean).collect())
}

/// Normalized, mean-removed autocorrelation of `segment` at a (rounded) `lag` in frames.
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

fn is_same_beat(a: f32, b: f32) -> bool {
    TEMPO_OCTAVES
        .iter()
        .any(|factor| (a / (b * factor) - 1.0).abs() <= TEMPO_MATCH_TOLERANCE)
}

fn ticks_of(seconds: f32) -> usize {
    (seconds / TICK_SECONDS).round() as usize
}

/// Do the old and new tempo confidently disagree?
fn tempo_disagrees(old: Tempo, new: Tempo, min_confidence: f32) -> bool {
    let (Some(old_bpm), Some(new_bpm)) = (old.bpm, new.bpm) else {
        return false;
    };
    let is_confident =
        new.confidence >= min_confidence && old.confidence >= MIN_OLD_TEMPO_CONFIDENCE;
    is_confident && !is_same_beat(old_bpm, new_bpm)
}

/// The tempo of the stretch before `end_tick` (exclusive) and after `window_start`, from the
/// longest recent-tempo length that fits, or `None` when none does.
fn old_tempo(prep: &Prepared, window_start: usize, end_tick: usize) -> Option<Tempo> {
    let available = end_tick.checked_sub(window_start)?;
    let index = RECENT_TEMPO_SECONDS
        .iter()
        .rposition(|&seconds| ticks_of(seconds) <= available)?;
    Some(prep.tempo(end_tick.checked_sub(1)?, index))
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TempoCheck {
    /// Into `RECENT_TEMPO_SECONDS`.
    pub recent_index: usize,
    pub min_confidence: f32,
}

/// Approach 2: compare the last few seconds with the rest of the window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RecentRule {
    pub recent_ticks: usize,
    /// Split when the old key's scale trails the recent best key by more than this. A gap only
    /// counts toward `sustain_ticks` while the recent best scale holds.
    pub key_gap: Option<f32>,
    /// Split when recent and old treble chroma correlate below this.
    pub chroma_correlation: Option<f32>,
    pub tempo: Option<TempoCheck>,
    pub min_tonal_seconds: f32,
    /// The old and the recent best key must each beat their best other-scale rival by this much.
    pub min_separation: f32,
    pub sustain_ticks: usize,
    pub min_old_ticks: usize,
}

/// Approach 3: Foote novelty peaks, optionally gated by a key or tempo change across the peak.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoveltyRule {
    pub half_width: usize,
    pub features: FeatureSet,
    pub threshold: f32,
    /// When set, a peak splits only if key or tempo differ on its two sides.
    pub gate: Option<Gate>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gate {
    pub key_gap: f32,
    pub tempo: Option<TempoCheck>,
    pub min_tonal_seconds: f32,
    /// Ticks after the peak before the gate is judged, and the last tick it may pass.
    pub min_after_ticks: usize,
    pub max_after_ticks: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FeatureSet {
    Chroma,
    Timbre,
    Rhythm,
    All,
}

impl FeatureSet {
    pub fn name(self) -> &'static str {
        match self {
            FeatureSet::Chroma => "chroma",
            FeatureSet::Timbre => "timbre",
            FeatureSet::Rhythm => "rhythm",
            FeatureSet::All => "all",
        }
    }
}

/// Approach 4: tempo steps and beat-phase jumps.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridRule {
    pub tempo: Option<TempoCheck>,
    /// Split on a beat-phase jump larger than this fraction of a beat.
    pub phase_jump: Option<f32>,
    pub min_salience: f32,
    pub sustain_ticks: usize,
    pub min_old_ticks: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Approach {
    Recent(RecentRule),
    Novelty(NoveltyRule),
    Grid(GridRule),
    /// The production `hunch_core::ChangeDetector`, run with the trace (`Trace::core_splits`).
    Core,
}

/// Runs `approach` over the trace. `novelty` must be the curve for a novelty rule's half width
/// and features (see `novelty_curve`); other approaches ignore it.
pub fn run_session(prep: &Prepared, approach: &Approach, novelty: &[f32]) -> Vec<Split> {
    match approach {
        Approach::Recent(rule) => run_recent(prep, rule),
        Approach::Novelty(rule) => run_novelty(prep, rule, novelty),
        Approach::Grid(rule) => run_grid(prep, rule),
        Approach::Core => prep.trace.core_splits.clone(),
    }
}

/// What the recent-vs-window comparison saw at one tick.
struct Disagreement {
    key: bool,
    tempo: bool,
    /// The best key on the recent stretch, when it had enough tonal evidence.
    recent_key: Option<usize>,
}

fn recent_disagreement(
    prep: &Prepared,
    rule: &RecentRule,
    window_start: usize,
    tick: usize,
) -> Disagreement {
    let none = Disagreement {
        key: false,
        tempo: false,
        recent_key: None,
    };
    let Some(recent_start) = (tick + 1).checked_sub(rule.recent_ticks) else {
        return none;
    };
    if recent_start < window_start + rule.min_old_ticks {
        return none;
    }
    let old = prep.evidence.range(window_start, recent_start);
    let recent = prep.evidence.range(recent_start, tick + 1);
    let has_tonal = old.tonal_seconds >= rule.min_tonal_seconds
        && recent.tonal_seconds >= rule.min_tonal_seconds;
    let mut key = false;
    let mut recent_key = None;
    if has_tonal {
        let old_scores = prep.scorer.scores(&old);
        let recent_scores = prep.scorer.scores(&recent);
        recent_key = recent_scores.as_ref().map(keys::best);
        if let (Some(threshold), Some(old_scores), Some(recent_scores)) =
            (rule.key_gap, old_scores, recent_scores)
        {
            let is_clear = keys::separation(&old_scores) >= rule.min_separation
                && keys::separation(&recent_scores) >= rule.min_separation;
            key |= is_clear && keys::scale_gap(&recent_scores, keys::best(&old_scores)) > threshold;
        }
        if let Some(threshold) = rule.chroma_correlation {
            key |= keys::correlation(&old.treble, &recent.treble) < threshold;
        }
    }
    let tempo = rule
        .tempo
        .is_some_and(|check| tempo_step(prep, check, window_start, tick));
    Disagreement {
        key,
        tempo,
        recent_key,
    }
}

/// Consecutive ticks a signal has disagreed, and the recent key the key streak follows.
#[derive(Default)]
struct Streaks {
    key: usize,
    key_scale: Option<usize>,
    tempo: usize,
}

impl Streaks {
    fn update(&mut self, seen: &Disagreement, is_stable_key_required: bool) {
        self.tempo = if seen.tempo { self.tempo + 1 } else { 0 };
        if !seen.key {
            (self.key, self.key_scale) = (0, None);
            return;
        }
        let is_same_scale = match (self.key_scale, seen.recent_key) {
            (Some(streak), Some(recent)) => streak == recent || keys::relative(streak) == recent,
            _ => false,
        };
        if is_stable_key_required && !is_same_scale {
            (self.key, self.key_scale) = (1, seen.recent_key);
            return;
        }
        self.key += 1;
        self.key_scale = self.key_scale.or(seen.recent_key);
    }
}

/// Does the recent tempo confidently differ from the tempo of the window before it?
fn tempo_step(prep: &Prepared, check: TempoCheck, window_start: usize, tick: usize) -> bool {
    let recent_ticks = ticks_of(RECENT_TEMPO_SECONDS[check.recent_index]);
    let Some(recent_start) = (tick + 1).checked_sub(recent_ticks) else {
        return false;
    };
    if recent_start < window_start {
        return false;
    }
    let Some(old) = old_tempo(prep, window_start, recent_start) else {
        return false;
    };
    tempo_disagrees(
        old,
        prep.tempo(tick, check.recent_index),
        check.min_confidence,
    )
}

fn run_recent(prep: &Prepared, rule: &RecentRule) -> Vec<Split> {
    let mut splits = Vec::new();
    let mut window_start = 0;
    let mut streaks = Streaks::default();
    for tick in 0..prep.len() {
        let seen = recent_disagreement(prep, rule, window_start, tick);
        streaks.update(&seen, rule.key_gap.is_some());
        let cause = if streaks.key >= rule.sustain_ticks {
            Cause::Key
        } else if streaks.tempo >= rule.sustain_ticks {
            Cause::Tempo
        } else {
            continue;
        };
        let run_start = tick + 1 - rule.sustain_ticks;
        let change_tick = match cause {
            Cause::Key => key_change_point(KeyChangeInput {
                prep,
                window_start,
                tick,
                recent_ticks: rule.recent_ticks,
                min_old_ticks: rule.min_old_ticks,
            }),
            _ => tempo_change_point(prep, window_start, tick, run_start),
        }
        .unwrap_or(
            run_start
                .saturating_sub(rule.recent_ticks / 2)
                .max(window_start),
        );
        splits.push(Split {
            detect_tick: tick,
            change_tick,
            cause,
        });
        window_start = change_tick;
        streaks = Streaks::default();
    }
    splits
}

struct KeyChangeInput<'a, 'b> {
    prep: &'a Prepared<'b>,
    window_start: usize,
    tick: usize,
    recent_ticks: usize,
    min_old_ticks: usize,
}

/// The tick after which the new key fits better than the old one, by a suffix-sum (CUSUM) search
/// over 1 s blocks since the window start.
fn key_change_point(input: KeyChangeInput) -> Option<usize> {
    let KeyChangeInput {
        prep,
        window_start,
        tick,
        recent_ticks,
        min_old_ticks,
    } = input;
    let recent_start = tick + 1 - recent_ticks;
    let old_scores = prep.scores(window_start, recent_start, 0.0)?;
    let new_scores = prep.scores(recent_start, tick + 1, 0.0)?;
    let (old_key, new_key) = (keys::best(&old_scores), keys::best(&new_scores));
    let scale_score =
        |scores: &[f32; KEY_COUNT], key: usize| scores[key].max(scores[keys::relative(key)]);
    let lowest = (window_start + min_old_ticks / 2)
        .max(recent_start.saturating_sub(ticks_of(LOOKBACK_SECONDS)));
    let mut best = None;
    let mut suffix = 0.0;
    let mut best_sum = f32::NEG_INFINITY;
    for start in (lowest..=tick).rev() {
        if let Some(scores) = prep.scores(start, start + 2, 0.0) {
            suffix += scale_score(&scores, new_key) - scale_score(&scores, old_key);
        }
        if suffix > best_sum {
            best_sum = suffix;
            best = Some(start);
        }
    }
    best
}

/// The tick after which onsets fit the new beat period better than the old one (CUSUM).
fn tempo_change_point(
    prep: &Prepared,
    window_start: usize,
    tick: usize,
    run_start: usize,
) -> Option<usize> {
    let new = prep.tempo(tick, 0).bpm?;
    let recent_start = run_start.saturating_sub(ticks_of(RECENT_TEMPO_SECONDS[0]));
    let old = old_tempo(prep, window_start, recent_start.max(window_start))?.bpm?;
    let frame_rate = features::frame_rate();
    let (old_lag, new_lag) = (60.0 / old * frame_rate, 60.0 / new * frame_rate);
    let half = flux_frame(TEMPO_CUSUM_HALF_SECONDS);
    let flux = &prep.trace.flux;
    let mut suffix = 0.0;
    let mut best = None;
    let mut best_sum = f32::NEG_INFINITY;
    let lowest = window_start.max(run_start.saturating_sub(ticks_of(LOOKBACK_SECONDS)));
    for start in (lowest..=tick).rev() {
        let center = flux_frame(Trace::seconds_of(start));
        let from = center.saturating_sub(half).min(flux.len());
        let to = (center + half).min(flux.len());
        let segment = &flux[from..to];
        suffix += autocorrelation(segment, new_lag) - autocorrelation(segment, old_lag);
        if suffix > best_sum {
            best_sum = suffix;
            best = Some(start);
        }
    }
    best
}

fn feature_rows<'a>(prep: &'a Prepared, set: FeatureSet) -> Vec<&'a [Vec<f32>]> {
    match set {
        FeatureSet::Chroma => vec![&prep.chroma],
        FeatureSet::Timbre => vec![&prep.timbre],
        FeatureSet::Rhythm => vec![&prep.rhythm],
        FeatureSet::All => vec![&prep.chroma, &prep.timbre, &prep.rhythm],
    }
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// Foote novelty at every tick: a Gaussian-tapered checkerboard kernel slid along the diagonal of
/// the self-similarity matrix. 0 where the kernel doesn't fit.
pub fn novelty_curve(prep: &Prepared, half_width: usize, set: FeatureSet) -> Vec<f32> {
    let rows = feature_rows(prep, set);
    let count = prep.len();
    let width = half_width as isize;
    let sigma = half_width as f32 / 2.0;
    let mut kernel = Vec::new();
    for i in -width..width {
        for j in -width..width {
            let sign = if (i < 0) == (j < 0) { 1.0 } else { -1.0 };
            let (x, y) = (i as f32 + 0.5, j as f32 + 0.5);
            kernel.push((
                i,
                j,
                sign * (-(x * x + y * y) / (2.0 * sigma * sigma)).exp(),
            ));
        }
    }
    let total: f32 = kernel.iter().map(|(_, _, weight)| weight.abs()).sum();
    (0..count)
        .map(|center| {
            if center < half_width || center + half_width > count {
                return 0.0;
            }
            let mut sum = 0.0;
            for &(i, j, weight) in &kernel {
                let a = (center as isize + i) as usize;
                let b = (center as isize + j) as usize;
                let similarity: f32 =
                    rows.iter().map(|row| dot(&row[a], &row[b])).sum::<f32>() / rows.len() as f32;
                sum += weight * similarity;
            }
            sum / total
        })
        .collect()
}

fn run_novelty(prep: &Prepared, rule: &NoveltyRule, curve: &[f32]) -> Vec<Split> {
    let half = rule.half_width;
    // A peak is confirmed once the curve is known half a kernel past it.
    let confirm_delay = half + half / 2;
    let mut splits = Vec::new();
    let mut window_start = 0;
    let mut pending: Vec<usize> = Vec::new();
    for tick in 0..prep.len() {
        if let Some(center) = tick.checked_sub(confirm_delay) {
            let is_peak = center >= window_start + half
                && curve[center] >= rule.threshold
                && (center.saturating_sub(half / 2)..=center + half / 2)
                    .all(|other| curve.get(other).is_none_or(|&value| value <= curve[center]));
            if is_peak {
                pending.push(center);
            }
        }
        let Some(gate) = rule.gate else {
            if let Some(&center) = pending.last() {
                splits.push(Split {
                    detect_tick: tick,
                    change_tick: center,
                    cause: Cause::Novelty,
                });
                window_start = center;
                pending.clear();
            }
            continue;
        };
        let passed = pending.iter().copied().find(|&center| {
            let after = tick.saturating_sub(center);
            after >= gate.min_after_ticks && gate_passes(prep, &gate, window_start, center, tick)
        });
        if let Some(center) = passed {
            splits.push(Split {
                detect_tick: tick,
                change_tick: center,
                cause: Cause::Novelty,
            });
            window_start = center;
            pending.clear();
            continue;
        }
        pending.retain(|&center| tick.saturating_sub(center) < gate.max_after_ticks);
    }
    splits
}

/// Do key or tempo differ between the window before `center` and the audio after it?
fn gate_passes(
    prep: &Prepared,
    gate: &Gate,
    window_start: usize,
    center: usize,
    tick: usize,
) -> bool {
    let old = prep.scores(window_start, center, gate.min_tonal_seconds);
    let new = prep.scores(center, tick + 1, gate.min_tonal_seconds);
    if let (Some(old), Some(new)) = (old, new) {
        if keys::scale_gap(&new, keys::best(&old)) > gate.key_gap {
            return true;
        }
    }
    let Some(check) = gate.tempo else {
        return false;
    };
    let after = tick + 1 - center;
    let Some(index) = RECENT_TEMPO_SECONDS
        .iter()
        .rposition(|&seconds| ticks_of(seconds) <= after)
        .filter(|&index| index >= check.recent_index)
    else {
        return false;
    };
    let Some(old) = old_tempo(prep, window_start, center) else {
        return false;
    };
    tempo_disagrees(old, prep.tempo(tick, index), check.min_confidence)
}

/// Beat phase (0..1 of a beat) and its salience over the last `PHASE_SECONDS` before `tick`.
fn beat_phase(flux: &[f32], tick: usize, lag_frames: f32) -> (f32, f32) {
    let end = flux_frame(Trace::seconds_of(tick)).min(flux.len());
    let start = end.saturating_sub(flux_frame(PHASE_SECONDS));
    let segment = &flux[start..end];
    if segment.is_empty() || lag_frames <= 0.0 {
        return (0.0, 0.0);
    }
    let mean = segment.iter().sum::<f32>() / segment.len() as f32;
    let (mut real, mut imaginary, mut magnitude) = (0.0, 0.0, 0.0);
    for (offset, &value) in segment.iter().enumerate() {
        let angle = TAU * (start + offset) as f32 / lag_frames;
        let centered = value - mean;
        real += centered * angle.cos();
        imaginary += centered * angle.sin();
        magnitude += centered.abs();
    }
    let salience = if magnitude > 0.0 {
        (real.hypot(imaginary)) / magnitude
    } else {
        0.0
    };
    (imaginary.atan2(real) / TAU, salience)
}

fn run_grid(prep: &Prepared, rule: &GridRule) -> Vec<Split> {
    let phase_ticks = ticks_of(PHASE_SECONDS);
    let mut splits = Vec::new();
    let mut window_start = 0;
    let mut run = 0;
    let mut phase_run = 0;
    for tick in 0..prep.len() {
        let old_start = window_start + rule.min_old_ticks;
        let mut is_phase_jump = false;
        if let (Some(limit), true) = (rule.phase_jump, tick >= old_start + phase_ticks) {
            let earlier = tick - phase_ticks;
            if let Some(bpm) =
                old_tempo(prep, window_start, earlier + 1).and_then(|tempo| tempo.bpm)
            {
                let lag = 60.0 / bpm * features::frame_rate();
                let (now, now_salience) = beat_phase(&prep.trace.flux, tick, lag);
                let (before, before_salience) = beat_phase(&prep.trace.flux, earlier, lag);
                let jump = (now - before + 0.5).rem_euclid(1.0) - 0.5;
                let is_salient =
                    now_salience >= rule.min_salience && before_salience >= rule.min_salience;
                is_phase_jump = is_salient && jump.abs() > limit;
            }
        }
        let is_tempo_step = tick >= old_start
            && rule
                .tempo
                .is_some_and(|check| tempo_step(prep, check, window_start, tick));
        if is_phase_jump || is_tempo_step {
            run += 1;
            phase_run += usize::from(is_phase_jump);
        } else {
            (run, phase_run) = (0, 0);
        }
        if run < rule.sustain_ticks {
            continue;
        }
        let run_start = tick + 1 - run;
        let is_phase = phase_run * 2 >= run;
        let change_tick = if is_phase {
            run_start.saturating_sub(phase_ticks / 2)
        } else {
            tempo_change_point(prep, window_start, tick, run_start)
                .unwrap_or(run_start.saturating_sub(phase_ticks / 2))
        }
        .max(window_start);
        let cause = if is_phase { Cause::Phase } else { Cause::Tempo };
        splits.push(Split {
            detect_tick: tick,
            change_tick,
            cause,
        });
        window_start = change_tick;
        (run, phase_run) = (0, 0);
    }
    splits
}
