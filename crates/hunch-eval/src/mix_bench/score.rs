//! Scores splits against the known change in each mix and the no-change single tracks.

use std::collections::BTreeMap;

use hunch_core::{Estimate, Key};

use super::detect::Split;
use super::pool::Category;
use super::trace::Trace;
use crate::metrics::{is_same_scale_as_any, is_tempo_match};

/// A detection this long after the change point still counts as catching it.
pub const HIT_WINDOW_SECONDS: f32 = 30.0;

#[derive(Debug, Clone)]
pub struct Truth {
    pub category: Category,
    pub kind: String,
    pub change_seconds: f32,
    pub fade_start: f32,
}

/// How one mix's splits relate to its change.
#[derive(Debug, Clone, Copy, Default)]
pub struct MixOutcome {
    /// Seconds from the change point to the detection, and the change-point error (estimate minus
    /// truth), for the first split in the hit window.
    pub hit: Option<(f32, f32)>,
    /// Splits before B was audible.
    pub early: usize,
    /// Further splits after the hit, or any split after the hit window.
    pub extra: usize,
}

pub fn classify(truth: &Truth, splits: &[Split]) -> MixOutcome {
    let mut outcome = MixOutcome::default();
    for split in splits {
        let detect = Trace::seconds_of(split.detect_tick);
        let change = Trace::seconds_of(split.change_tick);
        let is_in_window =
            detect >= truth.fade_start && detect <= truth.change_seconds + HIT_WINDOW_SECONDS;
        if detect < truth.fade_start {
            outcome.early += 1;
        } else if is_in_window && outcome.hit.is_none() {
            outcome.hit = Some((detect - truth.change_seconds, change - truth.change_seconds));
        } else {
            outcome.extra += 1;
        }
    }
    outcome
}

/// Recall and timing over a group of changing mixes.
#[derive(Debug, Clone, Default)]
pub struct HitStats {
    pub mixes: usize,
    pub hits: usize,
    pub delays: Vec<f32>,
    pub errors: Vec<f32>,
    pub extra: usize,
    pub early: usize,
}

impl HitStats {
    pub fn add(&mut self, outcome: &MixOutcome) {
        self.mixes += 1;
        self.extra += outcome.extra;
        self.early += outcome.early;
        if let Some((delay, error)) = outcome.hit {
            self.hits += 1;
            self.delays.push(delay);
            self.errors.push(error);
        }
    }

    pub fn recall(&self) -> f32 {
        self.hits as f32 / self.mixes.max(1) as f32
    }
}

pub fn quantile(values: &[f32], q: f32) -> Option<f32> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f32::total_cmp);
    let index = ((sorted.len() - 1) as f32 * q).round() as usize;
    Some(sorted[index])
}

pub fn format_optional(value: Option<f32>) -> String {
    value.map_or("—".to_string(), |value| format!("{value:+.1}"))
}

/// Every score of one detector config.
#[derive(Debug, Clone, Default)]
pub struct Summary {
    pub changes: HitStats,
    pub by_category: BTreeMap<Category, HitStats>,
    pub by_kind: BTreeMap<String, HitStats>,
    /// Same key and tempo pairs: splits there are neither required nor harmful.
    pub same_pairs: HitStats,
    pub negative_splits: usize,
    pub negative_hours: f32,
    /// Splits before the transition in every mix, over the hours of A played.
    pub early_splits: usize,
    pub a_hours: f32,
}

impl Summary {
    pub fn false_per_hour(&self) -> f32 {
        self.negative_splits as f32 / self.negative_hours.max(1e-6)
    }

    pub fn early_per_hour(&self) -> f32 {
        self.early_splits as f32 / self.a_hours.max(1e-6)
    }
}

pub struct SummaryInput<'a> {
    pub truths: &'a [Truth],
    pub mix_splits: &'a [Vec<Split>],
    pub negative_splits: &'a [Vec<Split>],
    pub negative_seconds: &'a [f32],
}

pub fn summarize(input: SummaryInput) -> Summary {
    let mut summary = Summary::default();
    for (truth, splits) in input.truths.iter().zip(input.mix_splits) {
        let outcome = classify(truth, splits);
        summary.early_splits += outcome.early;
        summary.a_hours += truth.fade_start / 3600.0;
        if !truth.category.is_change() {
            summary.same_pairs.add(&outcome);
            continue;
        }
        summary.changes.add(&outcome);
        summary
            .by_category
            .entry(truth.category)
            .or_default()
            .add(&outcome);
        summary
            .by_kind
            .entry(truth.kind.clone())
            .or_default()
            .add(&outcome);
    }
    summary.negative_splits = input.negative_splits.iter().map(Vec::len).sum();
    summary.negative_hours = input.negative_seconds.iter().sum::<f32>() / 3600.0;
    summary
}

/// What a readout got right against a track's labels.
#[derive(Debug, Clone, Copy, Default)]
pub struct Accuracy {
    pub key_right: usize,
    pub key_total: usize,
    pub bpm_right: usize,
    pub bpm_total: usize,
}

impl Accuracy {
    pub fn add(&mut self, estimate: Option<&Estimate>, keys: &[Key], bpm: Option<f32>) {
        let key = estimate.and_then(|estimate| estimate.key);
        self.key_total += 1;
        self.key_right += usize::from(key.is_some_and(|key| is_same_scale_as_any(key, keys)));
        if let Some(bpm) = bpm {
            self.bpm_total += 1;
            let predicted = estimate.and_then(|estimate| estimate.bpm);
            self.bpm_right +=
                usize::from(predicted.is_some_and(|predicted| is_tempo_match(predicted, bpm)));
        }
    }

    pub fn merge(&mut self, other: &Self) {
        self.key_right += other.key_right;
        self.key_total += other.key_total;
        self.bpm_right += other.bpm_right;
        self.bpm_total += other.bpm_total;
    }

    pub fn key(&self) -> f32 {
        self.key_right as f32 / self.key_total.max(1) as f32
    }

    pub fn bpm(&self) -> f32 {
        self.bpm_right as f32 / self.bpm_total.max(1) as f32
    }
}
