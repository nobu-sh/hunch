//! Key detector parameter sweep: decode each keyed/atonal sound once, run a grid of configs.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use clap::Args;
use hunch_core::tonal::{KeyDetector, KeyDetectorConfig, KeyProfile};
use hunch_core::{Key, Mode, PitchClass};
use serde::Deserialize;

use crate::fsl10k::{KeyLabel, Sound, Split};
use crate::metrics::{self, AtonalScore, KeyScore};
use crate::resample::{self, ANALYSIS_SAMPLE_RATE};
use crate::{DatasetArgs, decode, run};

const PITCH_CLASSES: u8 = 12;
const FIFTH_SEMITONES: u8 = 7;
const RELATIVE_MINOR_SEMITONES: u8 = 9;
/// Upper edges of the loop-length bins in the error analysis, in seconds.
const LOOP_SECONDS_EDGES: [f32; 4] = [2.0, 4.0, 8.0, 16.0];
/// Upper edges of the |tuning| bins in the error analysis, in cents.
const TUNING_CENTS_EDGES: [f32; 3] = [10.0, 25.0, 40.0];
const INSTRUMENTS: [&str; 6] = ["percussion", "bass", "chords", "melody", "fx", "vocal"];

#[derive(Args)]
pub struct KeySweepArgs {
    #[command(flatten)]
    dataset: DatasetArgs,

    /// Key profiles to try, comma-separated (krumhanslkessler, temperley, edma, shaath).
    #[arg(long, value_delimiter = ',', value_parser = parse_profile)]
    profile: Vec<KeyProfile>,

    /// Tonalness thresholds, applied after the run (no re-analysis per threshold).
    #[arg(long, value_delimiter = ',')]
    tonalness_threshold: Vec<f32>,

    #[arg(long, value_delimiter = ',')]
    bass_weight: Vec<f32>,

    #[arg(long, value_delimiter = ',')]
    tonic_weight: Vec<f32>,

    #[arg(long, value_delimiter = ',')]
    low_hz: Vec<f32>,

    #[arg(long, value_delimiter = ',')]
    high_hz: Vec<f32>,

    #[arg(long, value_delimiter = ',')]
    bass_cutoff_hz: Vec<f32>,

    #[arg(long, value_delimiter = ',')]
    overtone_weight: Vec<f32>,

    #[arg(long, value_delimiter = ',')]
    harmonicity_exponent: Vec<f32>,

    #[arg(long, value_delimiter = ',')]
    magnitude_compression: Vec<f32>,

    /// Highest acceptable atonal false-key rate when picking the best row.
    #[arg(long, default_value_t = 0.15)]
    max_false_key: f64,

    /// Print an error breakdown for the first config at the first threshold.
    #[arg(long)]
    analyze: bool,

    /// Fit the confidence model for the first config and threshold on train, check it on val
    /// (ignores `--split`).
    #[arg(long)]
    calibrate: bool,
}

fn parse_profile(name: &str) -> Result<KeyProfile, String> {
    KeyProfile::ALL
        .into_iter()
        .find(|profile| format!("{profile:?}").eq_ignore_ascii_case(name.trim()))
        .ok_or_else(|| format!("unknown key profile {name:?}"))
}

/// Every combination of the listed values, falling back to the default for unlisted parameters.
fn config_grid(args: &KeySweepArgs) -> Vec<KeyDetectorConfig> {
    let base = KeyDetectorConfig {
        tonalness_threshold: 0.0,
        ..KeyDetectorConfig::default()
    };
    let mut grid = vec![base];
    grid = expand(grid, &args.bass_weight, |c, v| c.bass_weight = v);
    grid = expand(grid, &args.tonic_weight, |c, v| c.tonic_weight = v);
    grid = expand(grid, &args.low_hz, |c, v| c.low_hz = v);
    grid = expand(grid, &args.high_hz, |c, v| c.high_hz = v);
    grid = expand(grid, &args.bass_cutoff_hz, |c, v| c.bass_cutoff_hz = v);
    grid = expand(grid, &args.overtone_weight, |c, v| c.overtone_weight = v);
    grid = expand(grid, &args.harmonicity_exponent, |c, v| {
        c.harmonicity_exponent = v
    });
    grid = expand(grid, &args.magnitude_compression, |c, v| {
        c.magnitude_compression = v
    });
    expand(grid, &args.profile, |c, v| c.profile = v)
}

/// Each config of `grid` once per value; `grid` unchanged when no values are listed.
fn expand<T: Copy>(
    grid: Vec<KeyDetectorConfig>,
    values: &[T],
    set: fn(&mut KeyDetectorConfig, T),
) -> Vec<KeyDetectorConfig> {
    if values.is_empty() {
        return grid;
    }
    grid.iter()
        .flat_map(|config| {
            values.iter().map(move |&value| {
                let mut config = *config;
                set(&mut config, value);
                config
            })
        })
        .collect()
}

fn describe(config: &KeyDetectorConfig) -> String {
    format!(
        "{:?} bass {} tonic {} {}-{}Hz cut {} ot {} hx {} mc {}",
        config.profile,
        config.bass_weight,
        config.tonic_weight,
        config.low_hz,
        config.high_hz,
        config.bass_cutoff_hz,
        config.overtone_weight,
        config.harmonicity_exponent,
        config.magnitude_compression
    )
}

/// What one config reported for one sound, before the tonalness threshold.
#[derive(Clone, Copy)]
struct Outcome {
    key: Option<Key>,
    confidence: f32,
    tonalness: f32,
    tuning_cents: f32,
    separation: f32,
    evidence: f32,
}

impl Outcome {
    fn key_at(&self, threshold: f32) -> Option<Key> {
        self.key.filter(|_| self.tonalness >= threshold)
    }
}

struct SoundOutcomes {
    loop_seconds: f32,
    /// One per config, in grid order.
    outcomes: Vec<Outcome>,
}

fn run_grid(samples: &[f32], grid: &[KeyDetectorConfig]) -> Vec<Outcome> {
    grid.iter()
        .map(|config| {
            let mut detector = KeyDetector::with_config(ANALYSIS_SAMPLE_RATE, *config);
            detector.push(samples);
            let estimate = detector.estimate();
            Outcome {
                key: estimate.key,
                confidence: estimate.confidence,
                tonalness: estimate.tonalness,
                tuning_cents: detector.tuning_cents(),
                separation: estimate.separation,
                evidence: estimate.evidence,
            }
        })
        .collect()
}

fn analyze_sound(
    sound: &Sound,
    min_seconds: f32,
    grid: &[KeyDetectorConfig],
) -> Result<SoundOutcomes, String> {
    let audio = decode::decode_mono(&sound.audio_path)?;
    let samples = resample::to_analysis_rate(&audio)?;
    if samples.is_empty() {
        return Err("no samples".to_string());
    }
    let loop_seconds = samples.len() as f32 / ANALYSIS_SAMPLE_RATE as f32;
    let min_len = (min_seconds.max(0.0) * ANALYSIS_SAMPLE_RATE as f32).ceil() as usize;
    let samples = resample::repeat_to_length(samples, min_len);
    Ok(SoundOutcomes {
        loop_seconds,
        outcomes: run_grid(&samples, grid),
    })
}

struct Scored<'a> {
    sound: &'a Sound,
    result: SoundOutcomes,
}

impl Scored<'_> {
    fn label(&self) -> Option<Key> {
        match self.sound.labels.key {
            KeyLabel::Key(key) => Some(key),
            _ => None,
        }
    }

    fn is_atonal(&self) -> bool {
        self.sound.labels.key == KeyLabel::Atonal
    }
}

struct RowScore {
    key: KeyScore,
    atonal: AtonalScore,
}

fn score_row(scored: &[&Scored], config_index: usize, threshold: f32) -> RowScore {
    let key = KeyScore::from_predictions(scored.iter().filter_map(|entry| {
        let label = entry.label()?;
        Some((entry.result.outcomes[config_index].key_at(threshold), label))
    }));
    let atonal = AtonalScore::from_predictions(
        scored
            .iter()
            .filter(|entry| entry.is_atonal())
            .map(|entry| entry.result.outcomes[config_index].key_at(threshold)),
    );
    RowScore { key, atonal }
}

fn pct(value: Option<f64>) -> String {
    value.map_or("-".to_string(), |v| format!("{:.1}", 100.0 * v))
}

pub fn run(args: &KeySweepArgs) -> Result<(), String> {
    let started = Instant::now();
    let (dataset, selected) = crate::load_selection(&args.dataset)?;
    let has_key_label =
        |sound: &&Sound| matches!(sound.labels.key, KeyLabel::Key(_) | KeyLabel::Atonal);
    let sounds: Vec<&Sound> = if args.calibrate {
        dataset
            .sounds
            .iter()
            .filter(|sound| matches!(sound.split, Split::Train | Split::Val))
            .filter(has_key_label)
            .collect()
    } else {
        selected
            .iter()
            .map(|&i| &dataset.sounds[i])
            .filter(has_key_label)
            .collect()
    };
    let grid = config_grid(args);
    let thresholds = if args.tonalness_threshold.is_empty() {
        vec![KeyDetectorConfig::default().tonalness_threshold]
    } else {
        args.tonalness_threshold.clone()
    };
    println!(
        "{} keyed/atonal sounds x {} configs x {} thresholds",
        sounds.len(),
        grid.len(),
        thresholds.len()
    );

    let min_seconds = args.dataset.min_seconds;
    let outcomes = run::par_map_sounds(&sounds, |sound| analyze_sound(sound, min_seconds, &grid));
    let mut scored = Vec::new();
    let mut failures = Vec::new();
    for (sound, outcome) in sounds.iter().copied().zip(outcomes) {
        match outcome {
            Ok(result) => scored.push(Scored { sound, result }),
            Err(error) => failures.push((sound, error)),
        }
    }
    let skipped = crate::summarize_failures(&failures);
    println!(
        "analyzed {} sounds, skipped {} {skipped:?} in {:.1}s\n",
        scored.len(),
        failures.len(),
        started.elapsed().as_secs_f32()
    );

    if args.calibrate {
        calibrate(&scored, thresholds[0]);
        return Ok(());
    }
    let all: Vec<&Scored> = scored.iter().collect();
    let hip_hop: Vec<&Scored> = scored.iter().filter(|s| s.sound.is_hip_hop).collect();
    print_table(&TableInput {
        all: &all,
        hip_hop: &hip_hop,
        grid: &grid,
        thresholds: &thresholds,
        max_false_key: args.max_false_key,
    });

    if args.analyze {
        let annotations = args
            .dataset
            .data
            .join("fsl10k-annotations")
            .join("annotations");
        let instrumentation = load_instrumentation(&annotations, &all)?;
        print_analysis(&all, thresholds[0], &instrumentation);
    }
    Ok(())
}

struct TableInput<'a> {
    all: &'a [&'a Scored<'a>],
    hip_hop: &'a [&'a Scored<'a>],
    grid: &'a [KeyDetectorConfig],
    thresholds: &'a [f32],
    max_false_key: f64,
}

fn print_table(input: &TableInput) {
    println!(
        "{:<58} {:>5} | {:>5} {:>5} {:>5} {:>5} | {:>5} {:>5} {:>5} | {:>7}",
        "config", "tonal", "cov", "same", "mirex", "fkey", "hh-sm", "hh-mx", "hh-fk", "vs row0"
    );
    let mut best: Option<(f64, String)> = None;
    for (index, config) in input.grid.iter().enumerate() {
        for &threshold in input.thresholds {
            let all = score_row(input.all, index, threshold);
            let hip_hop = score_row(input.hip_hop, index, threshold);
            let (gained, lost) =
                paired_same_scale(input.all, (0, input.thresholds[0]), (index, threshold));
            let line = format!(
                "{:<58} {:>5.3} | {:>5} {:>5} {:>5} {:>5} | {:>5} {:>5} {:>5} | +{gained}/-{lost}",
                describe(config),
                threshold,
                pct(all.key.coverage),
                pct(all.key.same_scale),
                pct(all.key.mirex),
                pct(all.atonal.false_key_rate),
                pct(hip_hop.key.same_scale),
                pct(hip_hop.key.mirex),
                pct(hip_hop.atonal.false_key_rate),
            );
            println!("{line}");
            let same_scale = all.key.same_scale.unwrap_or(0.0);
            let is_allowed = all.atonal.false_key_rate.unwrap_or(0.0) <= input.max_false_key;
            if is_allowed && best.as_ref().is_none_or(|(score, _)| same_scale > *score) {
                best = Some((same_scale, line));
            }
        }
    }
    match best {
        Some((_, line)) => println!(
            "\nbest same-scale with false-key <= {:.0}%:\n{line}",
            100.0 * input.max_false_key
        ),
        None => println!("\nno row meets the false-key limit"),
    }
}

/// Keyed sounds that are same-scale under `candidate` but not `reference`, and the reverse.
/// Each side is a (config index, tonalness threshold) pair.
fn paired_same_scale(
    scored: &[&Scored],
    reference: (usize, f32),
    candidate: (usize, f32),
) -> (usize, usize) {
    let is_right = |entry: &Scored, (index, threshold): (usize, f32)| {
        let label = entry.label();
        let key = entry.result.outcomes[index].key_at(threshold);
        matches!((key, label), (Some(key), Some(label)) if metrics::is_same_scale(key, label))
    };
    let mut counts = (0, 0);
    for entry in scored.iter().filter(|entry| entry.label().is_some()) {
        match (is_right(entry, reference), is_right(entry, candidate)) {
            (false, true) => counts.0 += 1,
            (true, false) => counts.1 += 1,
            _ => {}
        }
    }
    counts
}

/// How a predicted key relates to the label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Relation {
    Exact,
    Relative,
    FifthUp,
    FifthDown,
    Parallel,
    /// Same mode as the relative of a fifth (e.g. C major labeled, E minor predicted).
    RelativeOfFifth,
    /// Same mode, or relative, one semitone off: what a wrong tuning estimate produces.
    Semitone,
    Other,
    Missing,
}

fn relation(predicted: Option<Key>, label: Key) -> Relation {
    let Some(predicted) = predicted else {
        return Relation::Missing;
    };
    let up = (predicted.tonic.index() + PITCH_CLASSES - label.tonic.index()) % PITCH_CLASSES;
    let same_mode = predicted.mode == label.mode;
    if predicted == label {
        Relation::Exact
    } else if metrics::is_same_scale(predicted, label) {
        Relation::Relative
    } else if same_mode && up == FIFTH_SEMITONES {
        Relation::FifthUp
    } else if same_mode && up == PITCH_CLASSES - FIFTH_SEMITONES {
        Relation::FifthDown
    } else if up == 0 {
        Relation::Parallel
    } else if is_relative_of_fifth(predicted, label) {
        Relation::RelativeOfFifth
    } else if is_semitone_off(predicted, label) {
        Relation::Semitone
    } else {
        Relation::Other
    }
}

fn is_semitone_off(predicted: Key, label: Key) -> bool {
    let tonic = predicted.tonic.index();
    [1, PITCH_CLASSES - 1].into_iter().any(|shift| {
        let Some(shifted) = PitchClass::new((tonic + shift) % PITCH_CLASSES) else {
            return false;
        };
        metrics::is_same_scale(
            Key {
                tonic: shifted,
                mode: predicted.mode,
            },
            label,
        )
    })
}

/// The prediction is the relative key of a fifth above or below the label.
fn is_relative_of_fifth(predicted: Key, label: Key) -> bool {
    if predicted.mode == label.mode {
        return false;
    }
    // Shift the prediction onto the label's mode through its relative, then test for a fifth.
    let relative_shift = match predicted.mode {
        Mode::Minor => PITCH_CLASSES - RELATIVE_MINOR_SEMITONES,
        Mode::Major => RELATIVE_MINOR_SEMITONES,
    };
    let tonic = (predicted.tonic.index() + relative_shift) % PITCH_CLASSES;
    let up = (tonic + PITCH_CLASSES - label.tonic.index()) % PITCH_CLASSES;
    up == FIFTH_SEMITONES || up == PITCH_CLASSES - FIFTH_SEMITONES
}

#[derive(Deserialize, Default)]
struct InstrumentationFile {
    /// Mostly booleans, but some annotators wrote strings like `"Other"`.
    #[serde(default)]
    instrumentation: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    discard: bool,
}

/// Instruments any non-discarding annotator marked present, per sound id.
fn load_instrumentation(
    dir: &Path,
    scored: &[&Scored],
) -> Result<BTreeMap<u64, Vec<&'static str>>, String> {
    let annotators: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|error| format!("cannot list {}: {error}", dir.display()))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_dir())
        .collect();
    let mut instruments = BTreeMap::new();
    for entry in scored {
        let id = entry.sound.id;
        let mut present: Vec<&'static str> = Vec::new();
        for annotator in &annotators {
            let path = annotator.join(format!("sound-{id}.json"));
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            let file: InstrumentationFile = serde_json::from_str(&text)
                .map_err(|error| format!("cannot parse {}: {error}", path.display()))?;
            if file.discard {
                continue;
            }
            for name in INSTRUMENTS {
                let is_present =
                    file.instrumentation.get(name) == Some(&serde_json::Value::Bool(true));
                if is_present && !present.contains(&name) {
                    present.push(name);
                }
            }
        }
        instruments.insert(id, present);
    }
    Ok(instruments)
}

/// Same-scale tally for one group of keyed sounds.
#[derive(Default)]
struct Tally {
    sounds: usize,
    same_scale: usize,
    missing: usize,
}

impl Tally {
    fn add(&mut self, relation: Relation) {
        self.sounds += 1;
        self.same_scale += usize::from(matches!(relation, Relation::Exact | Relation::Relative));
        self.missing += usize::from(relation == Relation::Missing);
    }

    fn print(&self, name: &str) {
        println!(
            "  {name:<22} n {:>4}  same-scale {:>5}%  missing {:>5}%",
            self.sounds,
            pct(metrics::ratio(self.same_scale, self.sounds)),
            pct(metrics::ratio(self.missing, self.sounds)),
        );
    }
}

fn bin_name(value: f32, edges: &[f32], unit: &str) -> String {
    let mut low = 0.0;
    for &edge in edges {
        if value < edge {
            return format!("{low}-{edge} {unit}");
        }
        low = edge;
    }
    format!(">= {low} {unit}")
}

fn print_analysis(
    scored: &[&Scored],
    threshold: f32,
    instrumentation: &BTreeMap<u64, Vec<&'static str>>,
) {
    println!("\nerror analysis, config 0 at tonalness {threshold}:");
    let keyed: Vec<(&Scored, Key, Outcome)> = scored
        .iter()
        .filter_map(|entry| Some((*entry, entry.label()?, entry.result.outcomes[0])))
        .collect();

    // Counts per relation, split by label mode: [major, minor].
    let mut relations: BTreeMap<Relation, [usize; 2]> = BTreeMap::new();
    let mut by_mode: BTreeMap<String, Tally> = BTreeMap::new();
    let mut by_instrument: BTreeMap<String, Tally> = BTreeMap::new();
    let mut by_length: BTreeMap<String, Tally> = BTreeMap::new();
    let mut by_tuning: BTreeMap<String, Tally> = BTreeMap::new();
    for (entry, label, outcome) in &keyed {
        let relation = relation(outcome.key_at(threshold), *label);
        relations.entry(relation).or_default()[usize::from(label.mode == Mode::Minor)] += 1;
        by_mode
            .entry(format!("label {:?}", label.mode))
            .or_default()
            .add(relation);
        let instruments = instrumentation
            .get(&entry.sound.id)
            .cloned()
            .unwrap_or_default();
        for name in &instruments {
            by_instrument
                .entry(format!("with {name}"))
                .or_default()
                .add(relation);
        }
        if !instruments.contains(&"percussion") {
            by_instrument
                .entry("no percussion".to_string())
                .or_default()
                .add(relation);
        }
        let only_drums_and_bass = instruments
            .iter()
            .all(|name| matches!(*name, "percussion" | "bass"));
        if only_drums_and_bass {
            by_instrument
                .entry("only percussion/bass".to_string())
                .or_default()
                .add(relation);
        }
        by_length
            .entry(bin_name(
                entry.result.loop_seconds,
                &LOOP_SECONDS_EDGES,
                "s",
            ))
            .or_default()
            .add(relation);
        by_tuning
            .entry(bin_name(
                outcome.tuning_cents.abs(),
                &TUNING_CENTS_EDGES,
                "cents",
            ))
            .or_default()
            .add(relation);
    }

    println!(
        "keyed sounds by relation of prediction to label ({}):",
        keyed.len()
    );
    for (relation, [major, minor]) in &relations {
        println!(
            "  {:<22} {:>4}  {:>5}%  (major labels {major}, minor labels {minor})",
            format!("{relation:?}"),
            major + minor,
            pct(metrics::ratio(major + minor, keyed.len()))
        );
    }
    let predicted_major_for_minor = keyed
        .iter()
        .filter(|(_, label, outcome)| {
            label.mode == Mode::Minor
                && outcome
                    .key_at(threshold)
                    .is_some_and(|key| key.mode == Mode::Major)
        })
        .count();
    println!("  minor labels predicted major: {predicted_major_for_minor}");
    for (title, groups) in [
        ("by label mode", &by_mode),
        ("by instrumentation", &by_instrument),
        ("by loop length", &by_length),
        ("by |tuning|", &by_tuning),
    ] {
        println!("{title}:");
        for (name, tally) in groups {
            tally.print(name);
        }
    }

    let atonal: Vec<(&Scored, Outcome)> = scored
        .iter()
        .filter(|entry| entry.is_atonal())
        .map(|entry| (*entry, entry.result.outcomes[0]))
        .collect();
    let mut false_keys: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for (entry, outcome) in &atonal {
        let has_key = usize::from(outcome.key_at(threshold).is_some());
        let instruments = instrumentation
            .get(&entry.sound.id)
            .cloned()
            .unwrap_or_default();
        let group = if instruments.is_empty() {
            "none marked".to_string()
        } else {
            instruments.join("+")
        };
        let counts = false_keys.entry(group).or_default();
        counts.0 += 1;
        counts.1 += has_key;
    }
    println!("atonal false keys by instrumentation ({}):", atonal.len());
    for (group, (sounds, keys)) in &false_keys {
        println!(
            "  {group:<30} n {sounds:>4}  false-key {:>5}%",
            pct(metrics::ratio(*keys, *sounds))
        );
    }

    let calibration = metrics::calibrate(keyed.iter().filter_map(|(_, label, outcome)| {
        let key = outcome.key_at(threshold)?;
        Some((outcome.confidence, metrics::is_same_scale(key, *label)))
    }));
    println!("confidence vs same-scale accuracy:");
    for bin in calibration {
        println!(
            "  {:.1}-{:.1}  n {:>4}  {:>5}%",
            bin.low,
            bin.high,
            bin.count,
            pct(bin.accuracy)
        );
    }
}

/// Gradient steps when fitting the confidence model; the loss is convex and the inputs are 0..=1.
const CONFIDENCE_FIT_ITERATIONS: usize = 20_000;
const CONFIDENCE_FIT_STEP: f64 = 2.0;
/// Keeps the log-likelihood finite.
const MIN_PROBABILITY: f64 = 1e-9;
/// Share of the fitting weight given to atonal sounds that got a key. FSL10K over-represents drum
/// loops; in the browser most audio is tonal.
const ATONAL_WEIGHT_SHARE: f64 = 0.2;
const RELIABILITY_BINS: usize = 10;

/// A sound that got a key: keyed (right or wrong scale) or atonal (always wrong).
struct CalibrationPoint {
    separation: f32,
    tonalness: f32,
    evidence: f32,
    is_correct: bool,
    is_atonal: bool,
    weight: f64,
}

/// Every keyed or atonal sound that gets a key at `threshold`, weighted so atonal sounds carry
/// `ATONAL_WEIGHT_SHARE` of the total.
fn calibration_points(scored: &[&Scored], threshold: f32) -> Vec<CalibrationPoint> {
    let mut points: Vec<CalibrationPoint> = scored
        .iter()
        .filter_map(|entry| {
            let outcome = entry.result.outcomes[0];
            let key = outcome.key_at(threshold)?;
            let is_correct = match entry.label() {
                Some(label) => metrics::is_same_scale(key, label),
                None if entry.is_atonal() => false,
                None => return None,
            };
            Some(CalibrationPoint {
                separation: outcome.separation,
                tonalness: outcome.tonalness,
                evidence: outcome.evidence,
                is_correct,
                is_atonal: entry.is_atonal(),
                weight: 1.0,
            })
        })
        .collect();
    let atonal = points.iter().filter(|point| point.is_atonal).count() as f64;
    let keyed = points.len() as f64 - atonal;
    if atonal > 0.0 {
        let atonal_weight = ATONAL_WEIGHT_SHARE / (1.0 - ATONAL_WEIGHT_SHARE) * keyed / atonal;
        for point in points.iter_mut().filter(|point| point.is_atonal) {
            point.weight = atonal_weight;
        }
    }
    points
}

/// `evidence · σ(bias + separation_weight · separation + tonalness_weight · tonalness)`, the
/// form `KeyDetector` uses for `confidence`.
#[derive(Debug, Clone, Copy)]
struct ConfidenceModel {
    bias: f64,
    separation_weight: f64,
    tonalness_weight: f64,
}

impl ConfidenceModel {
    fn confidence(&self, point: &CalibrationPoint) -> f64 {
        let logit = self.bias
            + self.separation_weight * f64::from(point.separation)
            + self.tonalness_weight * f64::from(point.tonalness);
        f64::from(point.evidence) / (1.0 + (-logit).exp())
    }
}

/// Weighted maximum-likelihood fit of `ConfidenceModel` to correctness, by gradient descent.
fn fit_confidence(points: &[CalibrationPoint]) -> ConfidenceModel {
    let total_weight: f64 = points.iter().map(|point| point.weight).sum();
    let mut model = ConfidenceModel {
        bias: 0.0,
        separation_weight: 0.0,
        tonalness_weight: 0.0,
    };
    if total_weight <= 0.0 {
        return model;
    }
    for _ in 0..CONFIDENCE_FIT_ITERATIONS {
        let mut gradient = [0.0f64; 3];
        for point in points {
            let evidence = f64::from(point.evidence);
            let sigmoid = model.confidence(point) / evidence.max(MIN_PROBABILITY);
            let probability = model
                .confidence(point)
                .clamp(MIN_PROBABILITY, 1.0 - MIN_PROBABILITY);
            let target = f64::from(u8::from(point.is_correct));
            // d(-log-likelihood)/dp · dp/dlogit, with p = evidence · σ(logit).
            let loss_slope = (probability - target) / (probability * (1.0 - probability));
            let pull = point.weight * loss_slope * evidence * sigmoid * (1.0 - sigmoid);
            let inputs = [1.0, f64::from(point.separation), f64::from(point.tonalness)];
            for (g, x) in gradient.iter_mut().zip(inputs) {
                *g += pull * x;
            }
        }
        let step = CONFIDENCE_FIT_STEP / total_weight;
        model.bias -= step * gradient[0];
        model.separation_weight -= step * gradient[1];
        model.tonalness_weight -= step * gradient[2];
    }
    model
}

/// Weighted reliability bins and expected calibration error (weighted mean |accuracy − confidence|).
fn print_reliability(
    name: &str,
    points: &[&CalibrationPoint],
    confidence_of: impl Fn(&CalibrationPoint) -> f64,
) {
    // Per bin: (sounds, weight, weighted confidence, weighted correct).
    let mut bins = [(0usize, 0.0f64, 0.0f64, 0.0f64); RELIABILITY_BINS];
    for point in points {
        let confidence = confidence_of(point);
        let bin = ((confidence.clamp(0.0, 1.0) * RELIABILITY_BINS as f64) as usize)
            .min(RELIABILITY_BINS - 1);
        bins[bin].0 += 1;
        bins[bin].1 += point.weight;
        bins[bin].2 += point.weight * confidence;
        bins[bin].3 += point.weight * f64::from(u8::from(point.is_correct));
    }
    println!("{name}:");
    let total_weight: f64 = bins.iter().map(|bin| bin.1).sum();
    let mut gap = 0.0;
    for (index, (sounds, weight, confidence, correct)) in bins.iter().enumerate() {
        if *sounds == 0 {
            continue;
        }
        gap += (correct - confidence).abs();
        println!(
            "  {:.1}-{:.1}  n {sounds:>4}  mean conf {:>5}%  same-scale {:>5}%",
            index as f64 / RELIABILITY_BINS as f64,
            (index + 1) as f64 / RELIABILITY_BINS as f64,
            pct(Some(confidence / weight)),
            pct(Some(correct / weight)),
        );
    }
    let ece = (total_weight > 0.0).then(|| gap / total_weight);
    println!("  expected calibration error {}%", pct(ece));
}

fn mean(values: impl IntoIterator<Item = f64>) -> Option<f64> {
    let (mut total, mut count) = (0.0, 0usize);
    for value in values {
        total += value;
        count += 1;
    }
    (count > 0).then(|| total / count as f64)
}

fn calibrate(scored: &[Scored], threshold: f32) {
    let split = |wanted: Split| -> Vec<&Scored> {
        scored
            .iter()
            .filter(|entry| entry.sound.split == wanted)
            .collect()
    };
    let train = calibration_points(&split(Split::Train), threshold);
    let model = fit_confidence(&train);
    println!(
        "confidence model fitted on {} train sounds (atonal weight share {ATONAL_WEIGHT_SHARE}):",
        train.len()
    );
    println!("  CONFIDENCE_BIAS: f32 = {:.3};", model.bias);
    println!(
        "  CONFIDENCE_SEPARATION_WEIGHT: f32 = {:.3};",
        model.separation_weight
    );
    println!(
        "  CONFIDENCE_TONALNESS_WEIGHT: f32 = {:.3};\n",
        model.tonalness_weight
    );

    let uncalibrated = |point: &CalibrationPoint| f64::from(point.separation * point.evidence);
    let calibrated = |point: &CalibrationPoint| model.confidence(point);
    for (name, wanted) in [("train", Split::Train), ("val", Split::Val)] {
        let points = calibration_points(&split(wanted), threshold);
        let everything: Vec<&CalibrationPoint> = points.iter().collect();
        let keyed: Vec<&CalibrationPoint> = points.iter().filter(|p| !p.is_atonal).collect();
        print_reliability(
            &format!("{name} keyed, separation x evidence"),
            &keyed,
            uncalibrated,
        );
        print_reliability(&format!("{name} keyed, calibrated"), &keyed, calibrated);
        print_reliability(
            &format!("{name} reweighted population, calibrated"),
            &everything,
            calibrated,
        );

        let mean_where = |wanted: fn(&CalibrationPoint) -> bool| {
            mean(points.iter().filter(|p| wanted(p)).map(calibrated))
        };
        println!(
            "{name} mean calibrated confidence: correct keys {}%, wrong-scale keyed {}%, atonal false keys {}%\n",
            pct(mean_where(|p| p.is_correct)),
            pct(mean_where(|p| !p.is_correct && !p.is_atonal)),
            pct(mean_where(|p| p.is_atonal)),
        );
    }
}
