//! Key detector parameter sweep: decode each keyed/atonal sound once, run a grid of configs, and
//! score every dataset separately.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use clap::Args;
use hunch_core::tonal::{KeyDetector, KeyDetectorConfig, KeyProfile};
use hunch_core::{Key, Mode, PitchClass};
use serde::{Deserialize, Serialize};

use crate::fit::{self, CalibrationSample};
use crate::fsl10k::Split;
use crate::key_corpus::{self, Corpus, CorpusArgs, KeyItem, Window};
use crate::metrics::{self, AtonalScore, KeyScore};
use crate::profile_fit::{self, FitSettings, ProfileModel, ProfileSample};
use crate::resample::ANALYSIS_SAMPLE_RATE;
use crate::{SplitChoice, run};

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
    corpus: CorpusArgs,

    /// Part of each Beatport/GiantSteps+ track to analyze.
    #[arg(long, value_enum, default_value = "30s")]
    window: Window,

    /// Key profiles to try, comma-separated (krumhanslkessler, temperley, edma, shaath, learned).
    #[arg(long, value_delimiter = ',', value_parser = parse_profile)]
    profile: Vec<KeyProfile>,

    /// Presence thresholds, applied after the run (no re-analysis per threshold).
    #[arg(long, value_delimiter = ',')]
    presence_threshold: Vec<f32>,

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

    /// Print an error breakdown per dataset for the first config at the first threshold.
    #[arg(long)]
    analyze: bool,

    /// Fit the confidence model for the first config and threshold on train, check it on val
    /// (ignores `--split`).
    #[arg(long)]
    calibrate: bool,

    /// Write the first config's outcome for every sound here.
    #[arg(long)]
    csv: Option<PathBuf>,

    /// Fit a 24-key profile on train chroma from the first config, once per L2 strength listed
    /// in `--profile-l2`, and score it on train and val (ignores `--split`).
    #[arg(long)]
    fit_profile: bool,

    /// Fit the key presence gate on train outcomes of the first config and score thresholds on
    /// train and val (ignores `--split`).
    #[arg(long)]
    fit_gate: bool,

    #[arg(long, value_delimiter = ',', default_values_t = [0.01])]
    profile_l2: Vec<f64>,

    /// Share of the profile fit's likelihood on the exact key; the rest is on the scale.
    #[arg(long, value_delimiter = ',', default_values_t = [0.5])]
    profile_exact_share: Vec<f64>,

    /// Fit the tonic weight along with the profile instead of keeping the config's.
    #[arg(long)]
    profile_tonic_weight: bool,

    /// Fit a bias toward minor keys along with the profile.
    #[arg(long)]
    profile_minor_bias: bool,

    /// Share of the profile fit's weight on FSL10K; the rest goes to the track datasets.
    #[arg(long, default_value_t = 0.5)]
    profile_fsl10k_share: f64,
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
        presence_threshold: 0.0,
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

/// What one config reported for one sound, before the presence threshold.
#[derive(Clone, Copy)]
pub struct Outcome {
    pub key: Option<Key>,
    pub confidence: f32,
    pub tonalness: f32,
    pub presence: f32,
    pub tuning_cents: f32,
    pub separation: f32,
    pub clarity: f32,
    pub evidence: f32,
    pub chroma: [f32; PITCH_CLASSES as usize],
    pub bass_chroma: [f32; PITCH_CLASSES as usize],
}

impl Outcome {
    pub fn key_at(&self, threshold: f32) -> Option<Key> {
        self.key.filter(|_| self.presence >= threshold)
    }
}

pub struct SoundOutcomes {
    /// Length of the decoded audio before looping or windowing.
    pub source_seconds: f32,
    /// One per config, in grid order.
    pub outcomes: Vec<Outcome>,
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
                presence: estimate.presence,
                tuning_cents: detector.tuning_cents(),
                separation: estimate.separation,
                clarity: estimate.clarity,
                evidence: estimate.evidence,
                chroma: detector.chroma(),
                bass_chroma: detector.bass_chroma(),
            }
        })
        .collect()
}

pub struct Scored<'a> {
    pub item: &'a KeyItem,
    pub result: SoundOutcomes,
}

impl Scored<'_> {
    pub fn labels(&self) -> Option<&[Key]> {
        self.item.labels()
    }

    pub fn is_atonal(&self) -> bool {
        self.item.is_atonal()
    }
}

/// Decodes every item once and runs every config of `grid` on it, dropping the audio afterwards.
pub fn analyze_items<'a>(
    items: &'a [KeyItem],
    window: Window,
    min_seconds: f32,
    grid: &[KeyDetectorConfig],
) -> Vec<Scored<'a>> {
    let outcomes = run::par_map(items, |item| {
        let prepared = key_corpus::prepare(item, window, min_seconds)?;
        Ok(SoundOutcomes {
            source_seconds: prepared.source_seconds,
            outcomes: run_grid(&prepared.samples, grid),
        })
    });
    let mut scored = Vec::new();
    let mut failures = Vec::new();
    for (item, outcome) in items.iter().zip(outcomes) {
        match outcome {
            Ok(result) => scored.push(Scored { item, result }),
            Err(error) => failures.push((item, error)),
        }
    }
    for (item, error) in failures.iter().take(crate::MAX_LISTED_FAILURES) {
        eprintln!(
            "warning: skipping {} {} ({}): {error}",
            item.corpus,
            item.id,
            item.audio_path.display()
        );
    }
    if !failures.is_empty() {
        println!("skipped {} sounds that failed to decode", failures.len());
    }
    scored
}

/// The corpora present in `scored`, in order.
pub fn corpora(scored: &[Scored]) -> Vec<Corpus> {
    let mut corpora: Vec<Corpus> = scored.iter().map(|entry| entry.item.corpus).collect();
    corpora.sort();
    corpora.dedup();
    corpora
}

pub struct RowScore {
    pub key: KeyScore,
    pub atonal: AtonalScore,
}

pub fn score_row(scored: &[&Scored], config_index: usize, threshold: f32) -> RowScore {
    let key = KeyScore::from_alternatives(scored.iter().filter_map(|entry| {
        let labels = entry.labels()?;
        Some((
            entry.result.outcomes[config_index].key_at(threshold),
            labels,
        ))
    }));
    let atonal = AtonalScore::from_predictions(
        scored
            .iter()
            .filter(|entry| entry.is_atonal())
            .map(|entry| entry.result.outcomes[config_index].key_at(threshold)),
    );
    RowScore { key, atonal }
}

pub fn pct(value: Option<f64>) -> String {
    value.map_or("-".to_string(), |v| format!("{:.1}", 100.0 * v))
}

pub fn run(args: &KeySweepArgs) -> Result<(), String> {
    let started = Instant::now();
    // Calibration fits on train and checks on val, whatever `--split` says.
    let needs_train_and_val = args.calibrate || args.fit_profile || args.fit_gate;
    let split = if needs_train_and_val {
        SplitChoice::All
    } else {
        args.corpus.split
    };
    let mut items = key_corpus::load_items(&args.corpus, split)?;
    if needs_train_and_val {
        items.retain(|item| matches!(item.split, Split::Train | Split::Val));
    }
    let grid = config_grid(args);
    let thresholds = if args.presence_threshold.is_empty() {
        vec![KeyDetectorConfig::default().presence_threshold]
    } else {
        args.presence_threshold.clone()
    };
    println!(
        "{} keyed/atonal sounds x {} configs x {} thresholds, window {}",
        items.len(),
        grid.len(),
        thresholds.len(),
        args.window
    );

    let scored = analyze_items(&items, args.window, args.corpus.min_seconds, &grid);
    println!(
        "analyzed {} sounds in {:.1}s\n",
        scored.len(),
        started.elapsed().as_secs_f32()
    );

    if let Some(path) = &args.csv {
        write_outcomes_csv(path, &scored)?;
        println!("wrote {}", path.display());
    }
    if args.calibrate {
        calibrate(&scored, thresholds[0]);
        return Ok(());
    }
    if args.fit_gate {
        return fit_gate(&scored);
    }
    if args.fit_profile {
        fit_profiles(&scored, &grid[0], thresholds[0], args);
        return Ok(());
    }
    print_table(&TableInput {
        scored: &scored,
        grid: &grid,
        thresholds: &thresholds,
        max_false_key: args.max_false_key,
    });

    if args.analyze {
        let annotations = args
            .corpus
            .data
            .join("fsl10k-annotations")
            .join("annotations");
        let fsl10k = subset(&scored, Corpus::Fsl10k);
        let instrumentation = load_instrumentation(&annotations, &fsl10k)?;
        for corpus in corpora(&scored) {
            println!("\n=== {corpus}");
            print_analysis(&subset(&scored, corpus), thresholds[0], &instrumentation);
        }
    }
    Ok(())
}

/// The entries of `scored` from `corpus`.
pub fn subset<'a>(scored: &'a [Scored<'a>], corpus: Corpus) -> Vec<&'a Scored<'a>> {
    scored
        .iter()
        .filter(|entry| entry.item.corpus == corpus)
        .collect()
}

struct TableInput<'a> {
    scored: &'a [Scored<'a>],
    grid: &'a [KeyDetectorConfig],
    thresholds: &'a [f32],
    max_false_key: f64,
}

/// One dataset's columns in the sweep table. FSL10K also gets false-key and hip hop columns.
struct CorpusColumns<'a> {
    corpus: Corpus,
    all: Vec<&'a Scored<'a>>,
    hip_hop: Vec<&'a Scored<'a>>,
}

impl CorpusColumns<'_> {
    fn header(&self) -> String {
        let name = self.corpus.to_string();
        match self.corpus {
            Corpus::Fsl10k => format!(
                "{name}: {:>5} {:>5} {:>5} {:>5} {:>5}",
                "cov", "same", "mirex", "fkey", "hh-sm"
            ),
            Corpus::Beatport | Corpus::Giantsteps => {
                format!("{name}: {:>5} {:>5} {:>5}", "same", "exact", "mirex")
            }
        }
    }

    /// The row's cells, padded to the header width, and the dataset's score.
    fn cells(&self, config_index: usize, threshold: f32) -> (String, RowScore) {
        let row = score_row(&self.all, config_index, threshold);
        let indent = " ".repeat(self.corpus.to_string().len() + 1);
        let text = match self.corpus {
            Corpus::Fsl10k => format!(
                "{indent} {:>5} {:>5} {:>5} {:>5} {:>5}",
                pct(row.key.coverage),
                pct(row.key.same_scale),
                pct(row.key.mirex),
                pct(row.atonal.false_key_rate),
                pct(score_row(&self.hip_hop, config_index, threshold)
                    .key
                    .same_scale),
            ),
            Corpus::Beatport | Corpus::Giantsteps => format!(
                "{indent} {:>5} {:>5} {:>5}",
                pct(row.key.same_scale),
                pct(row.key.exact),
                pct(row.key.mirex),
            ),
        };
        (text, row)
    }
}

fn print_table(input: &TableInput) {
    let columns: Vec<CorpusColumns> = corpora(input.scored)
        .into_iter()
        .map(|corpus| {
            let all = subset(input.scored, corpus);
            let hip_hop = all
                .iter()
                .copied()
                .filter(|entry| entry.item.is_hip_hop)
                .collect();
            CorpusColumns {
                corpus,
                all,
                hip_hop,
            }
        })
        .collect();
    let headers: Vec<String> = columns.iter().map(CorpusColumns::header).collect();
    println!(
        "{:<58} {:>5} | {} | {:>7}",
        "config",
        "tonal",
        headers.join(" | "),
        "vs row0"
    );
    let all: Vec<&Scored> = input.scored.iter().collect();
    let mut best: Option<(f64, String)> = None;
    for (index, config) in input.grid.iter().enumerate() {
        for &threshold in input.thresholds {
            let mut cells = Vec::new();
            let mut same_scales = Vec::new();
            let mut is_allowed = true;
            for column in &columns {
                let (text, row) = column.cells(index, threshold);
                cells.push(text);
                same_scales.push(row.key.same_scale.unwrap_or(0.0));
                is_allowed &= row.atonal.false_key_rate.unwrap_or(0.0) <= input.max_false_key;
            }
            let (gained, lost) =
                paired_same_scale(&all, (0, input.thresholds[0]), (index, threshold));
            let line = format!(
                "{:<58} {:>5.3} | {} | +{gained}/-{lost}",
                describe(config),
                threshold,
                cells.join(" | "),
            );
            println!("{line}");
            // Datasets count equally, so the larger one doesn't decide alone.
            let mean_same_scale = same_scales.iter().sum::<f64>() / same_scales.len().max(1) as f64;
            if is_allowed
                && best
                    .as_ref()
                    .is_none_or(|(score, _)| mean_same_scale > *score)
            {
                best = Some((mean_same_scale, line));
            }
        }
    }
    match best {
        Some((_, line)) => println!(
            "\nbest mean same-scale over datasets with false-key <= {:.0}%:\n{line}",
            100.0 * input.max_false_key
        ),
        None => println!("\nno row meets the false-key limit"),
    }
}

/// Keyed sounds that are same-scale under `candidate` but not `reference`, and the reverse.
/// Each side is a (config index, presence threshold) pair.
fn paired_same_scale(
    scored: &[&Scored],
    reference: (usize, f32),
    candidate: (usize, f32),
) -> (usize, usize) {
    let is_right = |entry: &Scored, (index, threshold): (usize, f32)| {
        let key = entry.result.outcomes[index].key_at(threshold);
        match (key, entry.labels()) {
            (Some(key), Some(labels)) => metrics::is_same_scale_as_any(key, labels),
            _ => false,
        }
    };
    let mut counts = (0, 0);
    for entry in scored.iter().filter(|entry| entry.labels().is_some()) {
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

/// The alternative of `labels` the prediction comes closest to, by `Relation` order.
fn closest_label(predicted: Option<Key>, labels: &[Key]) -> Option<Key> {
    labels
        .iter()
        .copied()
        .min_by_key(|&label| relation(predicted, label))
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
        let id = entry.item.id;
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

/// Mean chroma and bass chroma per label mode, rotated so index 0 is the labeled tonic.
fn print_mean_chroma(keyed: &[(&Scored, Key, Outcome)]) {
    for mode in [Mode::Major, Mode::Minor] {
        let mut treble = [0.0f32; PITCH_CLASSES as usize];
        let mut bass = [0.0f32; PITCH_CLASSES as usize];
        let mut count = 0usize;
        for (_, label, outcome) in keyed.iter().filter(|(_, label, _)| label.mode == mode) {
            count += 1;
            let tonic = usize::from(label.tonic.index());
            for interval in 0..PITCH_CLASSES as usize {
                let pitch = (tonic + interval) % PITCH_CLASSES as usize;
                treble[interval] += outcome.chroma[pitch];
                bass[interval] += outcome.bass_chroma[pitch];
            }
        }
        let format = |values: &[f32; PITCH_CLASSES as usize]| {
            values
                .iter()
                .map(|value| format!("{:.2}", value / count.max(1) as f32))
                .collect::<Vec<_>>()
                .join(" ")
        };
        println!("  mean chroma from the labeled tonic, {mode} labels (n {count}):");
        println!("    treble {}", format(&treble));
        println!("    bass   {}", format(&bass));
    }
}

fn print_analysis(
    scored: &[&Scored],
    threshold: f32,
    instrumentation: &BTreeMap<u64, Vec<&'static str>>,
) {
    println!("\nerror analysis, config 0 at presence {threshold}:");
    let keyed: Vec<(&Scored, Key, Outcome)> = scored
        .iter()
        .filter_map(|entry| {
            let outcome = entry.result.outcomes[0];
            let label = closest_label(outcome.key_at(threshold), entry.labels()?)?;
            Some((*entry, label, outcome))
        })
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
            .get(&entry.item.id)
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
                entry.result.source_seconds,
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
    print_mean_chroma(&keyed);
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
            .get(&entry.item.id)
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
pub const ATONAL_WEIGHT_SHARE: f64 = 0.2;
const CONFIDENCE_INPUTS: usize = 3;
const RELIABILITY_BINS: usize = 10;
const GATE_FIT_L2: f64 = 1e-3;
/// Presence thresholds from 0 to 1 in this many steps are scored after fitting the gate.
const GATE_THRESHOLD_STEPS: usize = 20;

/// A sound that got a key: keyed (right or wrong scale) or atonal (always wrong).
struct CalibrationPoint {
    /// Separation, tonalness and clarity, in `ConfidenceModel::weights` order.
    inputs: [f64; CONFIDENCE_INPUTS],
    evidence: f32,
    is_correct: bool,
    is_atonal: bool,
    weight: f64,
}

/// Every keyed or atonal sound that gets a key at `threshold`, weighted so atonal sounds carry
/// `ATONAL_WEIGHT_SHARE` of the total. Pass one dataset at a time so the share holds within it.
fn calibration_points(scored: &[&Scored], threshold: f32) -> Vec<CalibrationPoint> {
    let mut points: Vec<CalibrationPoint> = scored
        .iter()
        .filter_map(|entry| {
            let outcome = entry.result.outcomes[0];
            let key = outcome.key_at(threshold)?;
            let is_correct = match entry.labels() {
                Some(labels) => metrics::is_same_scale_as_any(key, labels),
                None if entry.is_atonal() => false,
                None => return None,
            };
            Some(CalibrationPoint {
                inputs: [outcome.separation, outcome.tonalness, outcome.clarity].map(f64::from),
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

/// `evidence · σ(bias + weights · inputs)`, the form `KeyDetector` uses for `confidence`.
#[derive(Debug, Clone, Copy)]
struct ConfidenceModel {
    bias: f64,
    weights: [f64; CONFIDENCE_INPUTS],
}

impl ConfidenceModel {
    fn confidence(&self, point: &CalibrationPoint) -> f64 {
        let logit = self.bias
            + self
                .weights
                .iter()
                .zip(&point.inputs)
                .map(|(weight, input)| weight * input)
                .sum::<f64>();
        f64::from(point.evidence) / (1.0 + (-logit).exp())
    }
}

/// Weighted maximum-likelihood fit of `ConfidenceModel` to correctness, by gradient descent.
fn fit_confidence(points: &[CalibrationPoint]) -> ConfidenceModel {
    let total_weight: f64 = points.iter().map(|point| point.weight).sum();
    let mut model = ConfidenceModel {
        bias: 0.0,
        weights: [0.0; CONFIDENCE_INPUTS],
    };
    if total_weight <= 0.0 {
        return model;
    }
    for _ in 0..CONFIDENCE_FIT_ITERATIONS {
        let mut gradient = [0.0f64; CONFIDENCE_INPUTS + 1];
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
            let inputs = std::iter::once(1.0).chain(point.inputs.iter().copied());
            for (g, x) in gradient.iter_mut().zip(inputs) {
                *g += pull * x;
            }
        }
        let step = CONFIDENCE_FIT_STEP / total_weight;
        model.bias -= step * gradient[0];
        for (weight, g) in model.weights.iter_mut().zip(&gradient[1..]) {
            *weight -= step * g;
        }
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
    for (index, (sounds, weight, confidence, correct)) in bins.iter().enumerate() {
        if *sounds == 0 {
            continue;
        }
        println!(
            "  {:.1}-{:.1}  n {sounds:>4}  mean conf {:>5}%  same-scale {:>5}%",
            index as f64 / RELIABILITY_BINS as f64,
            (index + 1) as f64 / RELIABILITY_BINS as f64,
            pct(Some(confidence / weight)),
            pct(Some(correct / weight)),
        );
    }
    let ece = metrics::expected_calibration_error(
        points
            .iter()
            .map(|point| (confidence_of(point), point.is_correct, point.weight)),
    );
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
    let split = |wanted: Split, corpus: Option<Corpus>| -> Vec<&Scored> {
        scored
            .iter()
            .filter(|entry| entry.item.split == wanted)
            .filter(|entry| corpus.is_none_or(|corpus| entry.item.corpus == corpus))
            .collect()
    };
    let train: Vec<CalibrationPoint> = corpora(scored)
        .into_iter()
        .flat_map(|corpus| calibration_points(&split(Split::Train, Some(corpus)), threshold))
        .collect();
    let model = fit_confidence(&train);
    println!(
        "confidence model fitted on {} train sounds of every dataset (atonal weight share {ATONAL_WEIGHT_SHARE} within FSL10K):",
        train.len()
    );
    println!("  CONFIDENCE_BIAS: f32 = {:.3};", model.bias);
    for (name, weight) in ["SEPARATION", "TONALNESS", "CLARITY"]
        .iter()
        .zip(model.weights)
    {
        println!("  CONFIDENCE_{name}_WEIGHT: f32 = {weight:.3};");
    }
    println!();

    let uncalibrated = |point: &CalibrationPoint| point.inputs[0] * f64::from(point.evidence);
    let calibrated = |point: &CalibrationPoint| model.confidence(point);
    for (split_name, wanted) in [("train", Split::Train), ("val", Split::Val)] {
        for corpus in corpora(scored) {
            let name = format!("{corpus} {split_name}");
            let points = calibration_points(&split(wanted, Some(corpus)), threshold);
            let everything: Vec<&CalibrationPoint> = points.iter().collect();
            let keyed: Vec<&CalibrationPoint> = points.iter().filter(|p| !p.is_atonal).collect();
            print_reliability(
                &format!("{name} keyed, separation x evidence"),
                &keyed,
                uncalibrated,
            );
            print_reliability(&format!("{name} keyed, calibrated"), &keyed, calibrated);
            if keyed.len() < everything.len() {
                print_reliability(
                    &format!("{name} reweighted population, calibrated"),
                    &everything,
                    calibrated,
                );
            }

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
}

fn profile_samples(scored: &[&Scored]) -> Vec<(usize, ProfileSample)> {
    scored
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| {
            let outcome = &entry.result.outcomes[0];
            let sample =
                ProfileSample::new(&outcome.chroma, &outcome.bass_chroma, entry.labels()?)?;
            Some((index, sample))
        })
        .collect()
}

/// Same-scale / exact / MIREX of `model` on `scored`, ungated and gated at `threshold`.
fn profile_scores(
    model: &ProfileModel,
    scored: &[&Scored],
    threshold: f32,
) -> (KeyScore, KeyScore) {
    let samples = profile_samples(scored);
    let predictions: Vec<(Key, &[Key], f32)> = samples
        .iter()
        .filter_map(|(index, sample)| {
            let entry = scored[*index];
            Some((
                model.predict(sample),
                entry.labels()?,
                entry.result.outcomes[0].presence,
            ))
        })
        .collect();
    let ungated = KeyScore::from_alternatives(
        predictions
            .iter()
            .map(|(key, labels, _)| (Some(*key), *labels)),
    );
    let gated =
        KeyScore::from_alternatives(predictions.iter().map(|(key, labels, presence)| {
            (Some(*key).filter(|_| *presence >= threshold), *labels)
        }));
    (ungated, gated)
}

fn print_profile_scores(name: &str, model: &ProfileModel, scored: &[Scored], threshold: f32) {
    let mut cells = Vec::new();
    for split in [Split::Train, Split::Val] {
        for corpus in corpora(scored) {
            let entries: Vec<&Scored> = subset(scored, corpus)
                .into_iter()
                .filter(|entry| entry.item.split == split)
                .collect();
            let (ungated, gated) = profile_scores(model, &entries, threshold);
            cells.push(format!(
                "{corpus} {split} {}/{}/{} gated {}",
                pct(ungated.same_scale),
                pct(ungated.exact),
                pct(ungated.mirex),
                pct(gated.same_scale)
            ));
        }
    }
    println!("{name:<16} {}", cells.join(" | "));
}

/// Fits the profile on train with each dataset weighted equally, then scores train and val.
fn fit_profiles(
    scored: &[Scored],
    config: &KeyDetectorConfig,
    threshold: f32,
    args: &KeySweepArgs,
) {
    let profile = config.profile;
    let Some(prior) = ProfileModel::from_hand(
        [profile.template(Mode::Major), profile.template(Mode::Minor)],
        f64::from(config.tonic_weight),
    ) else {
        eprintln!("error: {profile:?} has a flat template");
        return;
    };
    let mut train = Vec::new();
    for corpus in corpora(scored) {
        let entries: Vec<&Scored> = subset(scored, corpus)
            .into_iter()
            .filter(|entry| entry.item.split == Split::Train)
            .collect();
        let mut samples: Vec<ProfileSample> = profile_samples(&entries)
            .into_iter()
            .map(|(_, s)| s)
            .collect();
        let share = match corpus {
            Corpus::Fsl10k => args.profile_fsl10k_share,
            Corpus::Beatport | Corpus::Giantsteps => 1.0 - args.profile_fsl10k_share,
        };
        let weight = share / samples.len().max(1) as f64;
        samples.iter_mut().for_each(|sample| sample.weight = weight);
        println!(
            "{corpus}: {} train sounds for the profile fit",
            samples.len()
        );
        train.extend(samples);
    }
    println!("same/exact/MIREX ungated, same-scale gated at presence {threshold}:");
    print_profile_scores(&format!("{profile:?}"), &prior, scored, threshold);
    let settings = args.profile_l2.iter().flat_map(|&l2| {
        args.profile_exact_share
            .iter()
            .map(move |&exact_share| FitSettings {
                l2,
                exact_share,
                learns_tonic_weight: args.profile_tonic_weight,
                learns_minor_bias: args.profile_minor_bias,
            })
    });
    for settings in settings {
        let report = profile_fit::fit(&train, &prior, &settings);
        let name = format!("l2 {} ex {}", settings.l2, settings.exact_share);
        print_profile_scores(&name, &report.model, scored, threshold);
        let [major, minor] = report.model.display_templates();
        let format = |template: [f64; 12]| {
            template
                .iter()
                .map(|value| format!("{value:.3}"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        println!(
            "  loss {:.4} after {} steps, scale {:.2}, tonic_weight {:.4}, minor_bias {:.4}",
            report.loss,
            report.iterations,
            report.model.scale,
            report.model.tonic_weight,
            report.model.minor_bias
        );
        println!("  major [{}]", format(major));
        println!("  minor [{}]", format(minor));
    }
}

#[derive(Serialize)]
struct OutcomeRow {
    corpus: String,
    id: u64,
    split: String,
    labels: String,
    predicted: String,
    same_scale: bool,
    exact: bool,
    tonalness: f32,
    presence: f32,
    separation: f32,
    clarity: f32,
    evidence: f32,
    confidence: f32,
    tuning_cents: f32,
}

fn write_outcomes_csv(path: &Path, scored: &[Scored]) -> Result<(), String> {
    let mut writer = csv::Writer::from_path(path)
        .map_err(|error| format!("cannot create {}: {error}", path.display()))?;
    for entry in scored {
        let outcome = &entry.result.outcomes[0];
        let labels = entry.labels().unwrap_or_default();
        let row = OutcomeRow {
            corpus: entry.item.corpus.to_string(),
            id: entry.item.id,
            split: entry.item.split.to_string(),
            labels: if entry.is_atonal() {
                "atonal".to_string()
            } else {
                labels
                    .iter()
                    .map(Key::to_string)
                    .collect::<Vec<_>>()
                    .join(" | ")
            },
            predicted: outcome.key.map(|key| key.to_string()).unwrap_or_default(),
            same_scale: outcome
                .key
                .is_some_and(|key| metrics::is_same_scale_as_any(key, labels)),
            exact: outcome.key.is_some_and(|key| labels.contains(&key)),
            tonalness: outcome.tonalness,
            presence: outcome.presence,
            separation: outcome.separation,
            clarity: outcome.clarity,
            evidence: outcome.evidence,
            confidence: outcome.confidence,
            tuning_cents: outcome.tuning_cents,
        };
        writer
            .serialize(row)
            .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    }
    writer
        .flush()
        .map_err(|error| format!("cannot write {}: {error}", path.display()))
}

/// Inputs of the key presence gate, in the order `KeyDetector` weights them.
fn gate_inputs(outcome: &Outcome) -> Vec<f64> {
    [outcome.tonalness, outcome.clarity].map(f64::from).to_vec()
}

/// Fits the presence gate on train: keyed sounds (any dataset) against FSL10K atonal ones, each
/// group carrying an equal share, then scores a range of thresholds on train and val.
fn fit_gate(scored: &[Scored]) -> Result<(), String> {
    let train: Vec<&Scored> = scored
        .iter()
        .filter(|entry| entry.item.split == Split::Train)
        .collect();
    let group_of = |entry: &Scored| (entry.item.corpus, entry.is_atonal());
    let mut group_sizes: BTreeMap<(Corpus, bool), usize> = BTreeMap::new();
    for entry in &train {
        *group_sizes.entry(group_of(entry)).or_default() += 1;
    }
    let samples: Vec<CalibrationSample> = train
        .iter()
        .filter(|entry| entry.result.outcomes[0].key.is_some())
        .map(|entry| CalibrationSample {
            inputs: gate_inputs(&entry.result.outcomes[0]),
            is_correct: !entry.is_atonal(),
            weight: 1.0 / group_sizes[&group_of(entry)] as f64,
        })
        .collect();
    let gate = fit::fit_calibration(&samples, GATE_FIT_L2)?;
    println!(
        "presence gate fitted on {} train sounds, groups {group_sizes:?}:",
        samples.len()
    );
    println!("  GATE_BIAS: f32 = {:.3};", gate.bias);
    for (name, weight) in ["TONALNESS", "CLARITY"].iter().zip(&gate.weights) {
        println!("  GATE_{name}_WEIGHT: f32 = {weight:.3};");
    }

    let presence = |entry: &Scored| -> f64 {
        let outcome = &entry.result.outcomes[0];
        if outcome.key.is_none() {
            return 0.0;
        }
        gate.probability(&gate_inputs(outcome))
    };
    for split in [Split::Train, Split::Val] {
        println!(
            "\n{split}: presence threshold, then per dataset coverage / same-scale (/ false-key)"
        );
        for step in 0..=GATE_THRESHOLD_STEPS {
            let threshold = step as f64 / GATE_THRESHOLD_STEPS as f64;
            let mut cells = Vec::new();
            for corpus in corpora(scored) {
                let entries: Vec<&Scored> = subset(scored, corpus)
                    .into_iter()
                    .filter(|entry| entry.item.split == split)
                    .collect();
                let gated = |entry: &&Scored| {
                    entry.result.outcomes[0]
                        .key
                        .filter(|_| presence(entry) >= threshold)
                };
                let key = KeyScore::from_alternatives(
                    entries
                        .iter()
                        .filter_map(|entry| Some((gated(entry), entry.labels()?))),
                );
                let atonal = AtonalScore::from_predictions(
                    entries.iter().filter(|entry| entry.is_atonal()).map(gated),
                );
                let false_key = if atonal.sounds > 0 {
                    format!(" / {}", pct(atonal.false_key_rate))
                } else {
                    String::new()
                };
                cells.push(format!(
                    "{corpus} {} / {}{false_key}",
                    pct(key.coverage),
                    pct(key.same_scale)
                ));
            }
            println!("  {threshold:.2}  {}", cells.join(" | "));
        }
    }
    Ok(())
}
