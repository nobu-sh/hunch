//! Per-sound scoring, the printed FSL10K report, and its CSV/JSON outputs.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use hunch_core::{Estimate, Key};
use serde::Serialize;

use crate::fsl10k::{ExpertTempo, KeyLabel, Sound, Split};
use crate::metrics::{
    self, AtonalScore, CalibrationBin, KeyScore, LockStats, TempoError, TempoScore,
};
use crate::run::SoundRun;

/// One analyzed sound and its labels.
pub struct Evaluated<'a> {
    pub sound: &'a Sound,
    pub run: SoundRun,
}

impl Evaluated<'_> {
    fn expert_bpm(&self) -> Option<f32> {
        match self.sound.labels.tempo_expert {
            ExpertTempo::Bpm(bpm) => Some(bpm),
            ExpertTempo::Unlabeled | ExpertTempo::Ambiguous => None,
        }
    }

    fn key_label(&self) -> Option<Key> {
        match self.sound.labels.key {
            KeyLabel::Key(key) => Some(key),
            KeyLabel::Unlabeled | KeyLabel::Atonal | KeyLabel::Ambiguous => None,
        }
    }

    fn creator_error(&self) -> Option<TempoError> {
        let label = self.sound.labels.tempo_creator?;
        Some(metrics::classify_tempo(self.run.last.bpm, label))
    }

    fn expert_error(&self) -> Option<TempoError> {
        Some(metrics::classify_tempo(
            self.run.last.bpm,
            self.expert_bpm()?,
        ))
    }

    fn is_same_scale(&self) -> Option<bool> {
        let label = self.key_label()?;
        Some(
            self.run
                .last
                .key
                .is_some_and(|key| metrics::is_same_scale(key, label)),
        )
    }

    fn tempo_lock_seconds(&self) -> Option<f32> {
        let label = self.sound.labels.tempo_creator?;
        let is_correct =
            |e: &Estimate| e.bpm.is_some_and(|bpm| metrics::is_tempo_match(bpm, label));
        metrics::lock_seconds(&self.run, is_correct)
    }

    fn key_lock_seconds(&self) -> Option<f32> {
        let label = self.key_label()?;
        let is_correct = |e: &Estimate| e.key.is_some_and(|key| metrics::is_same_scale(key, label));
        metrics::lock_seconds(&self.run, is_correct)
    }
}

#[derive(Debug, Serialize)]
pub struct SubsetReport {
    pub name: String,
    pub sounds: usize,
    pub tempo_creator: TempoScore,
    pub tempo_expert: TempoScore,
    pub key: KeyScore,
    pub atonal: AtonalScore,
    /// `bpm_confidence` vs Acc1 against the creator label.
    pub bpm_calibration: Vec<CalibrationBin>,
    /// `key_confidence` vs same-scale accuracy on keyed sounds.
    pub key_calibration: Vec<CalibrationBin>,
    pub tempo_lock: LockStats,
    pub key_lock: LockStats,
}

impl SubsetReport {
    pub fn build(name: &str, results: &[&Evaluated]) -> Self {
        let creator: Vec<(&Evaluated, TempoError)> = results
            .iter()
            .filter_map(|r| Some((*r, r.creator_error()?)))
            .collect();
        let keyed: Vec<(&Evaluated, Key)> = results
            .iter()
            .filter_map(|r| Some((*r, r.key_label()?)))
            .collect();
        let atonal = results
            .iter()
            .filter(|r| r.sound.labels.key == KeyLabel::Atonal);

        Self {
            name: name.to_string(),
            sounds: results.len(),
            tempo_creator: TempoScore::from_errors(creator.iter().map(|(_, error)| *error)),
            tempo_expert: TempoScore::from_errors(results.iter().filter_map(|r| r.expert_error())),
            key: KeyScore::from_predictions(
                keyed.iter().map(|(r, label)| (r.run.last.key, *label)),
            ),
            atonal: AtonalScore::from_predictions(atonal.map(|r| r.run.last.key)),
            bpm_calibration: metrics::calibrate(
                creator
                    .iter()
                    .map(|(r, error)| (r.run.last.bpm_confidence, *error == TempoError::Correct)),
            ),
            key_calibration: metrics::calibrate(keyed.iter().map(|(r, label)| {
                let is_correct = r
                    .run
                    .last
                    .key
                    .is_some_and(|key| metrics::is_same_scale(key, *label));
                (r.run.last.key_confidence, is_correct)
            })),
            tempo_lock: LockStats::from_seconds(
                results.iter().filter_map(|r| r.tempo_lock_seconds()),
            ),
            key_lock: LockStats::from_seconds(results.iter().filter_map(|r| r.key_lock_seconds())),
        }
    }

    pub fn print(&self) {
        println!("\n=== {} ({} sounds) ===", self.name, self.sounds);
        print_tempo("Tempo vs creator labels", &self.tempo_creator);
        print_tempo("Tempo vs expert labels", &self.tempo_expert);
        let key = &self.key;
        println!(
            "Key ({} keyed sounds): coverage {}, MIREX {}, same-scale {}, exact {}, miss rate {}",
            key.sounds,
            percent(key.coverage),
            score(key.mirex),
            percent(key.same_scale),
            percent(key.exact),
            percent(key.miss_rate),
        );
        println!(
            "Key presence ({} atonal sounds): false-key rate {}",
            self.atonal.sounds,
            percent(self.atonal.false_key_rate)
        );
        print_calibration("bpm_confidence vs Acc1", &self.bpm_calibration);
        print_calibration("key_confidence vs same-scale", &self.key_calibration);
        print_lock("Tempo lock (Acc1 vs creator)", &self.tempo_lock);
        print_lock("Key lock (same-scale)", &self.key_lock);
    }
}

fn print_tempo(title: &str, tempo: &TempoScore) {
    let errors = &tempo.errors;
    println!(
        "{title} ({} sounds): coverage {}, Acc1 {}, Acc2 {}",
        tempo.sounds,
        percent(tempo.coverage),
        percent(tempo.acc1),
        percent(tempo.acc2),
    );
    println!(
        "  errors: half {}, double {}, third {}, triple {}, other {}, missing {}",
        errors.half, errors.double, errors.third, errors.triple, errors.other, errors.missing
    );
}

fn print_calibration(title: &str, bins: &[CalibrationBin]) {
    println!("Calibration, {title}:");
    for bin in bins.iter().filter(|bin| bin.count > 0) {
        println!(
            "  {:.1}-{:.1}: {:>5} sounds, accuracy {}",
            bin.low,
            bin.high,
            bin.count,
            percent(bin.accuracy)
        );
    }
}

fn print_lock(title: &str, lock: &LockStats) {
    let median = lock
        .median_seconds
        .map_or("—".to_string(), |seconds| format!("{seconds:.1}s"));
    println!(
        "{title}: median {median} over {} sounds ending correct",
        lock.sounds_ending_correct
    );
}

fn percent(value: Option<f64>) -> String {
    value.map_or("—".to_string(), |v| format!("{:.1}%", v * 100.0))
}

fn score(value: Option<f64>) -> String {
    value.map_or("—".to_string(), |v| format!("{v:.3}"))
}

/// How many sounds carry each kind of label.
#[derive(Debug, Default, Serialize)]
pub struct LabelStats {
    pub sounds: usize,
    pub creators: usize,
    pub tempo_creator: usize,
    pub tempo_expert: usize,
    pub tempo_expert_ambiguous: usize,
    pub key: usize,
    pub atonal: usize,
    pub key_ambiguous: usize,
    pub hip_hop: usize,
}

impl LabelStats {
    pub fn count<'a>(sounds: impl IntoIterator<Item = &'a Sound>) -> Self {
        let mut stats = Self::default();
        let mut creators = BTreeSet::new();
        for sound in sounds {
            let labels = &sound.labels;
            stats.sounds += 1;
            creators.insert(sound.username.as_str());
            stats.tempo_creator += usize::from(labels.tempo_creator.is_some());
            stats.tempo_expert += usize::from(matches!(labels.tempo_expert, ExpertTempo::Bpm(_)));
            stats.tempo_expert_ambiguous +=
                usize::from(labels.tempo_expert == ExpertTempo::Ambiguous);
            stats.key += usize::from(matches!(labels.key, KeyLabel::Key(_)));
            stats.atonal += usize::from(labels.key == KeyLabel::Atonal);
            stats.key_ambiguous += usize::from(labels.key == KeyLabel::Ambiguous);
            stats.hip_hop += usize::from(sound.is_hip_hop);
        }
        stats.creators = creators.len();
        stats
    }
}

/// Label counts for each split and overall.
pub fn label_stats_by_split(sounds: &[Sound]) -> BTreeMap<String, LabelStats> {
    let mut stats: BTreeMap<String, LabelStats> = Split::ALL
        .iter()
        .map(|split| {
            let in_split = sounds.iter().filter(|sound| sound.split == *split);
            (split.to_string(), LabelStats::count(in_split))
        })
        .collect();
    stats.insert("all".to_string(), LabelStats::count(sounds));
    stats
}

pub fn print_label_stats(stats: &BTreeMap<String, LabelStats>) {
    println!(
        "{:<6} {:>6} {:>8} {:>8} {:>8} {:>9} {:>6} {:>7} {:>9} {:>8}",
        "split",
        "sounds",
        "creators",
        "tempo_c",
        "tempo_e",
        "tempo_e?",
        "key",
        "atonal",
        "key?",
        "hip_hop"
    );
    for (name, s) in stats {
        println!(
            "{:<6} {:>6} {:>8} {:>8} {:>8} {:>9} {:>6} {:>7} {:>9} {:>8}",
            name,
            s.sounds,
            s.creators,
            s.tempo_creator,
            s.tempo_expert,
            s.tempo_expert_ambiguous,
            s.key,
            s.atonal,
            s.key_ambiguous,
            s.hip_hop
        );
    }
    println!(
        "(tempo_c = creator BPM, tempo_e = agreeing expert BPM, tempo_e? = ambiguous expert BPM, key? = ambiguous key)"
    );
}

#[derive(Serialize)]
struct CsvRow<'a> {
    id: u64,
    split: String,
    genres: String,
    hip_hop: bool,
    creator_bpm: Option<f32>,
    expert_bpm: Option<f32>,
    key_label: String,
    bpm: Option<f32>,
    bpm_confidence: f32,
    bpm_alternate: Option<f32>,
    key: Option<String>,
    key_confidence: f32,
    seconds_analyzed: f32,
    tempo_error: Option<&'a str>,
    tempo_acc1: Option<bool>,
    tempo_acc2: Option<bool>,
    expert_tempo_error: Option<&'a str>,
    key_mirex: Option<f64>,
    key_same_scale: Option<bool>,
    tempo_lock_seconds: Option<f32>,
    key_lock_seconds: Option<f32>,
}

/// One row per sound, for digging into misses.
pub fn write_csv(path: &Path, results: &[Evaluated]) -> Result<(), String> {
    let mut writer = csv::Writer::from_path(path)
        .map_err(|error| format!("cannot create {}: {error}", path.display()))?;
    for result in results {
        let last = &result.run.last;
        let creator_error = result.creator_error();
        let genres: Vec<&str> = result.sound.genres.iter().map(String::as_str).collect();
        let key_mirex = result
            .key_label()
            .map(|label| last.key.map_or(0.0, |key| metrics::mirex_score(key, label)));
        let row = CsvRow {
            id: result.sound.id,
            split: result.sound.split.to_string(),
            genres: genres.join("|"),
            hip_hop: result.sound.is_hip_hop,
            creator_bpm: result.sound.labels.tempo_creator,
            expert_bpm: result.expert_bpm(),
            key_label: result.sound.labels.key.to_string(),
            bpm: last.bpm,
            bpm_confidence: last.bpm_confidence,
            bpm_alternate: last.bpm_alternate,
            key: last.key.map(|key| key.to_string()),
            key_confidence: last.key_confidence,
            seconds_analyzed: last.seconds_analyzed,
            tempo_error: creator_error.map(TempoError::name),
            tempo_acc1: creator_error.map(|error| error == TempoError::Correct),
            tempo_acc2: creator_error.map(TempoError::is_octave_correct),
            expert_tempo_error: result.expert_error().map(TempoError::name),
            key_mirex,
            key_same_scale: result.is_same_scale(),
            tempo_lock_seconds: result.tempo_lock_seconds(),
            key_lock_seconds: result.key_lock_seconds(),
        };
        writer
            .serialize(row)
            .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    }
    writer
        .flush()
        .map_err(|error| format!("cannot write {}: {error}", path.display()))
}

/// Machine-readable summary so runs can be diffed.
#[derive(Serialize)]
pub struct Summary<'a> {
    pub split: String,
    pub min_seconds: f32,
    pub selected: usize,
    pub evaluated: usize,
    pub skipped: BTreeMap<String, usize>,
    pub labels: &'a BTreeMap<String, LabelStats>,
    pub subsets: Vec<SubsetReport>,
}

pub fn write_json(path: &Path, summary: &Summary) -> Result<(), String> {
    let file =
        File::create(path).map_err(|error| format!("cannot create {}: {error}", path.display()))?;
    serde_json::to_writer_pretty(BufWriter::new(file), summary)
        .map_err(|error| format!("cannot write {}: {error}", path.display()))
}
