//! Octave-candidate feature dump and the `fit-octave` command around `fit`.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use hunch_core::rhythm::{OCTAVE_FEATURE_NAMES, OctaveCandidate, TempoDetector};
use serde::Serialize;

use crate::fit::{self, CandidateSet, FitOptions, OctaveModel};
use crate::fsl10k::{ExpertTempo, Sound};
use crate::metrics;
use crate::resample::ANALYSIS_SAMPLE_RATE;
use crate::run;

/// Columns before the feature columns in an octave-features CSV.
const FIXED_COLUMNS: [&str; 7] = [
    "id",
    "split",
    "candidate_bpm",
    "label_bpm",
    "expert_bpm",
    "is_correct",
    "hip_hop",
];
const ID_COLUMN: usize = 0;
const IS_CORRECT_COLUMN: usize = 5;
const MAX_ITERATIONS: usize = 5_000;
const GRADIENT_TOLERANCE: f64 = 1e-6;

/// The candidates `TempoDetector` proposes for one sound.
pub struct SoundCandidates<'a> {
    pub sound: &'a Sound,
    pub candidates: Vec<OctaveCandidate>,
}

/// Pushes `samples` through a fresh `TempoDetector` in extension-sized chunks.
pub fn candidates_for(samples: &[f32]) -> Vec<OctaveCandidate> {
    let mut detector = TempoDetector::new(ANALYSIS_SAMPLE_RATE);
    for chunk in samples.chunks(run::chunk_len()) {
        detector.push(chunk);
    }
    detector.octave_candidates()
}

/// Fails if any candidate's feature vector doesn't line up with `OCTAVE_FEATURE_NAMES`.
pub fn check_feature_lengths(sounds: &[SoundCandidates]) -> Result<(), String> {
    let expected = OCTAVE_FEATURE_NAMES.len();
    for entry in sounds {
        if let Some(bad) = entry
            .candidates
            .iter()
            .find(|c| c.features.len() != expected)
        {
            return Err(format!(
                "sound {}: candidate at {:.1} BPM has {} features but OCTAVE_FEATURE_NAMES has {expected}",
                entry.sound.id,
                bad.bpm,
                bad.features.len()
            ));
        }
    }
    Ok(())
}

/// Writes one row per candidate; returns the number of rows written.
pub fn write_features_csv(path: &Path, sounds: &[SoundCandidates]) -> Result<usize, String> {
    let write_error = |error: csv::Error| format!("cannot write {}: {error}", path.display());
    let mut writer = csv::Writer::from_path(path)
        .map_err(|error| format!("cannot create {}: {error}", path.display()))?;
    let header = FIXED_COLUMNS.iter().chain(OCTAVE_FEATURE_NAMES);
    writer.write_record(header).map_err(write_error)?;

    let mut rows = 0;
    for entry in sounds {
        let sound = entry.sound;
        let Some(label) = sound.labels.tempo_creator else {
            continue;
        };
        let expert = match sound.labels.tempo_expert {
            ExpertTempo::Bpm(bpm) => bpm.to_string(),
            ExpertTempo::Unlabeled | ExpertTempo::Ambiguous => String::new(),
        };
        for candidate in &entry.candidates {
            let is_correct = metrics::is_tempo_match(candidate.bpm, label);
            let mut record = vec![
                sound.id.to_string(),
                sound.split.to_string(),
                candidate.bpm.to_string(),
                label.to_string(),
                expert.clone(),
                u8::from(is_correct).to_string(),
                u8::from(sound.is_hip_hop).to_string(),
            ];
            record.extend(candidate.features.iter().map(f32::to_string));
            writer.write_record(&record).map_err(write_error)?;
            rows += 1;
        }
    }
    writer
        .flush()
        .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    Ok(rows)
}

/// Candidate sets read back from an octave-features CSV.
pub struct FeatureTable {
    pub feature_names: Vec<String>,
    pub sets: Vec<CandidateSet>,
}

pub fn read_features_csv(path: &Path) -> Result<FeatureTable, String> {
    let read_error = |error: csv::Error| format!("cannot read {}: {error}", path.display());
    let mut reader = csv::Reader::from_path(path).map_err(read_error)?;
    let header = reader.headers().map_err(read_error)?.clone();
    let columns: Vec<&str> = header.iter().collect();
    if columns.len() < FIXED_COLUMNS.len() || columns[..FIXED_COLUMNS.len()] != FIXED_COLUMNS {
        return Err(format!(
            "{}: header must start with {}, got {}",
            path.display(),
            FIXED_COLUMNS.join(","),
            columns.join(",")
        ));
    }
    let feature_names: Vec<String> = columns[FIXED_COLUMNS.len()..]
        .iter()
        .map(|name| name.to_string())
        .collect();

    let mut by_id: BTreeMap<u64, CandidateSet> = BTreeMap::new();
    for (index, record) in reader.records().enumerate() {
        let record = record.map_err(read_error)?;
        let line = index + 2;
        let field = |column: usize| record.get(column).unwrap_or_default();
        let id: u64 = parse_field(field(ID_COLUMN), path, line)?;
        let is_correct = parse_field::<u8>(field(IS_CORRECT_COLUMN), path, line)? == 1;
        let features = (FIXED_COLUMNS.len()..columns.len())
            .map(|column| parse_field::<f64>(field(column), path, line))
            .collect::<Result<Vec<f64>, String>>()?;
        let set = by_id.entry(id).or_insert_with(|| CandidateSet {
            id,
            features: Vec::new(),
            is_correct: Vec::new(),
        });
        set.features.push(features);
        set.is_correct.push(is_correct);
    }
    Ok(FeatureTable {
        feature_names,
        sets: by_id.into_values().collect(),
    })
}

fn parse_field<T: std::str::FromStr>(text: &str, path: &Path, line: usize) -> Result<T, String>
where
    T::Err: std::fmt::Display,
{
    text.trim()
        .parse()
        .map_err(|error| format!("{}:{line}: cannot parse {text:?}: {error}", path.display()))
}

pub struct FitOctaveArgs<'a> {
    pub train: &'a Path,
    pub val: &'a Path,
    pub out: &'a Path,
    pub l2: f64,
}

#[derive(Serialize)]
struct WeightsFile<'a> {
    feature_names: &'a [String],
    mean: &'a [f64],
    std: &'a [f64],
    weights: &'a [f64],
    l2: f64,
    train_acc: Option<f64>,
    val_acc: Option<f64>,
    train_sounds: usize,
    val_sounds: usize,
    skipped_no_correct_train: usize,
    skipped_no_correct_val: usize,
}

/// Loads the CSVs, fits, prints accuracies and weights, and writes the weights JSON.
pub fn fit_octave(args: &FitOctaveArgs) -> Result<(), String> {
    let train = read_features_csv(args.train)?;
    let val = read_features_csv(args.val)?;
    if train.feature_names != val.feature_names {
        return Err(format!(
            "feature columns differ: train has [{}], val has [{}]",
            train.feature_names.join(","),
            val.feature_names.join(",")
        ));
    }
    if train.feature_names.is_empty() {
        return Err(format!(
            "{} has no feature columns: OCTAVE_FEATURE_NAMES is empty, so there is nothing to fit",
            args.train.display()
        ));
    }

    let (train_sets, skipped_train) = keep_fittable(train.sets);
    let (val_sets, skipped_val) = keep_fittable(val.sets);
    println!(
        "train: {} sounds ({skipped_train} skipped: no correct candidate)",
        train_sets.len()
    );
    println!(
        "val:   {} sounds ({skipped_val} skipped: no correct candidate)",
        val_sets.len()
    );
    if train_sets.is_empty() {
        return Err(format!(
            "{} has no sounds with a correct candidate to fit",
            args.train.display()
        ));
    }

    let options = FitOptions {
        l2: args.l2,
        max_iterations: MAX_ITERATIONS,
        tolerance: GRADIENT_TOLERANCE,
    };
    let result = fit::fit(&train_sets, train.feature_names.len(), &options)?;
    let model = &result.model;
    let train_acc = model.accuracy(&train_sets);
    let val_acc = model.accuracy(&val_sets);
    println!(
        "fit: {} iterations, loss {:.4}, l2 {}",
        result.iterations, result.loss, args.l2
    );
    println!(
        "octave accuracy: train {}, val {}",
        format_accuracy(train_acc),
        format_accuracy(val_acc)
    );
    print_weights(&train.feature_names, model);

    let weights_file = WeightsFile {
        feature_names: &train.feature_names,
        mean: &model.mean,
        std: &model.std,
        weights: &model.weights,
        l2: args.l2,
        train_acc,
        val_acc,
        train_sounds: train_sets.len(),
        val_sounds: val_sets.len(),
        skipped_no_correct_train: skipped_train,
        skipped_no_correct_val: skipped_val,
    };
    let file = File::create(args.out)
        .map_err(|error| format!("cannot create {}: {error}", args.out.display()))?;
    serde_json::to_writer_pretty(BufWriter::new(file), &weights_file)
        .map_err(|error| format!("cannot write {}: {error}", args.out.display()))?;
    println!("wrote {}", args.out.display());
    println!("\n{}", rust_snippet(&train.feature_names, model));
    Ok(())
}

fn keep_fittable(sets: Vec<CandidateSet>) -> (Vec<CandidateSet>, usize) {
    let (fittable, skipped): (Vec<CandidateSet>, Vec<CandidateSet>) =
        sets.into_iter().partition(CandidateSet::has_correct);
    (fittable, skipped.len())
}

fn format_accuracy(accuracy: Option<f64>) -> String {
    accuracy.map_or("—".to_string(), |a| format!("{:.1}%", a * 100.0))
}

fn print_weights(names: &[String], model: &OctaveModel) {
    println!(
        "{:<32} {:>10} {:>10} {:>10}",
        "feature", "weight", "mean", "std"
    );
    for (index, name) in names.iter().enumerate() {
        println!(
            "{:<32} {:>10.4} {:>10.4} {:>10.4}",
            name, model.weights[index], model.mean[index], model.std[index]
        );
    }
}

/// Constants to paste next to `OCTAVE_FEATURE_NAMES` in `hunch-core`.
fn rust_snippet(names: &[String], model: &OctaveModel) -> String {
    let list = |values: &[f64]| {
        values
            .iter()
            .map(|v| format!("{:?}", *v as f32))
            .collect::<Vec<_>>()
            .join(", ")
    };
    format!(
        "// Fitted by `hunch-eval fit-octave` over features [{}].\n\
         pub const OCTAVE_FEATURE_MEAN: &[f32] = &[{}];\n\
         pub const OCTAVE_FEATURE_STD: &[f32] = &[{}];\n\
         pub const OCTAVE_WEIGHTS: &[f32] = &[{}];",
        names.join(", "),
        list(&model.mean),
        list(&model.std),
        list(&model.weights)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_csv(name: &str, contents: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(name);
        std::fs::write(&path, contents).expect("write csv");
        path
    }

    #[test]
    fn reads_candidates_grouped_by_sound() {
        let path = temp_csv(
            "hunch-eval-octave-read.csv",
            "id,split,candidate_bpm,label_bpm,expert_bpm,is_correct,hip_hop,a,b\n\
             1,train,70,140,,0,1,0.5,1\n\
             1,train,140,140,140,1,1,0.25,2\n\
             2,train,90,90,,1,0,1,3\n",
        );
        let table = read_features_csv(&path).expect("read");
        std::fs::remove_file(&path).ok();

        assert_eq!(table.feature_names, vec!["a", "b"]);
        assert_eq!(table.sets.len(), 2);
        assert_eq!(
            table.sets[0].features,
            vec![vec![0.5, 1.0], vec![0.25, 2.0]]
        );
        assert_eq!(table.sets[0].is_correct, vec![false, true]);
        assert_eq!(table.sets[1].id, 2);
    }

    #[test]
    fn rejects_unexpected_header() {
        let path = temp_csv("hunch-eval-octave-bad-header.csv", "id,bpm\n1,90\n");
        let error = read_features_csv(&path).err().expect("error");
        std::fs::remove_file(&path).ok();
        assert!(error.contains("header must start with"), "{error}");
    }

    #[test]
    fn reports_unparseable_fields_with_line() {
        let path = temp_csv(
            "hunch-eval-octave-bad-field.csv",
            "id,split,candidate_bpm,label_bpm,expert_bpm,is_correct,hip_hop,a\n\
             1,train,70,140,,0,1,oops\n",
        );
        let error = read_features_csv(&path).err().expect("error");
        std::fs::remove_file(&path).ok();
        assert!(error.contains(":2:") && error.contains("oops"), "{error}");
    }
}
