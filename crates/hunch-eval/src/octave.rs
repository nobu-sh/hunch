//! Octave-candidate feature dump and the `fit-octave` command around `fit`.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use hunch_core::rhythm::{
    CONFIDENCE_INPUT_NAMES, OCTAVE_FEATURE_NAMES, OctaveAnalysis, TempoDetector, confidence_inputs,
};
use serde::Serialize;

use crate::fit::{self, Calibration, CalibrationSample, CandidateSet, FitOptions, OctaveModel};
use crate::fsl10k::{ExpertTempo, Sound};
use crate::metrics;
use crate::resample::ANALYSIS_SAMPLE_RATE;
use crate::run;

/// Columns before the feature columns in an octave-features CSV.
const FIXED_COLUMNS: [&str; 9] = [
    "id",
    "split",
    "candidate_bpm",
    "label_bpm",
    "expert_bpm",
    "is_correct",
    "hip_hop",
    "periodicity",
    "loop_weight",
];
const ID_COLUMN: usize = 0;
const IS_CORRECT_COLUMN: usize = 5;
const PERIODICITY_COLUMN: usize = 7;
const LOOP_WEIGHT_COLUMN: usize = 8;
const MAX_ITERATIONS: usize = 5_000;
const GRADIENT_TOLERANCE: f64 = 1e-6;
/// Light regularization so a near-constant confidence input can't make the fit singular.
const CALIBRATION_L2: f64 = 1e-3;
const RELIABILITY_BINS: usize = 10;
/// Top of the generated `octave_weights.rs`.
const WEIGHTS_MODULE_HEADER: &str = "\
//! Octave-decision weights, fitted by `hunch-eval fit-octave --rust-out`.
//!
//! Format: three `[f32; N]` arrays and one `f32`, all literals. Entry `i` of each array belongs to
//! feature `OCTAVE_FEATURE_NAMES[i]`, one per line with the name in a trailing comment. A
//! candidate scores `BIAS + Σ WEIGHTS[i] · (feature[i] − FEATURE_MEANS[i]) / FEATURE_STDS[i]`, and
//! the softmax of the scores over a sound's candidates is the octave probability. Every std must be
//! positive. `CONFIDENCE_WEIGHTS` (one per `CONFIDENCE_INPUT_NAMES`) and `CONFIDENCE_BIAS` map the
//! winner's evidence to the reported confidence through a logistic.
";

/// What `TempoDetector` decides one sound's tempo from.
pub struct SoundCandidates<'a> {
    pub sound: &'a Sound,
    pub analysis: OctaveAnalysis,
}

/// Pushes `samples` through a fresh `TempoDetector` in extension-sized chunks. No candidates when
/// the detector has none (too short or silent).
pub fn analysis_for(samples: &[f32]) -> OctaveAnalysis {
    let mut detector = TempoDetector::new(ANALYSIS_SAMPLE_RATE);
    for chunk in samples.chunks(run::chunk_len()) {
        detector.push(chunk);
    }
    detector.octave_analysis().unwrap_or(OctaveAnalysis {
        candidates: Vec::new(),
        periodicity: 0.0,
        loop_weight: 0.0,
    })
}

/// Fails if any candidate's feature vector doesn't line up with `OCTAVE_FEATURE_NAMES`.
pub fn check_feature_lengths(sounds: &[SoundCandidates]) -> Result<(), String> {
    let expected = OCTAVE_FEATURE_NAMES.len();
    for entry in sounds {
        if let Some(bad) = entry
            .analysis
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
        let analysis = &entry.analysis;
        for candidate in &analysis.candidates {
            let is_correct = metrics::is_tempo_match(candidate.bpm, label);
            let mut record = vec![
                sound.id.to_string(),
                sound.split.to_string(),
                candidate.bpm.to_string(),
                label.to_string(),
                expert.clone(),
                u8::from(is_correct).to_string(),
                u8::from(sound.is_hip_hop).to_string(),
                analysis.periodicity.to_string(),
                analysis.loop_weight.to_string(),
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
    /// Each sound's confidence evidence besides the octave probability, by id.
    pub rhythm: BTreeMap<u64, RhythmEvidence>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RhythmEvidence {
    pub periodicity: f64,
    pub loop_weight: f64,
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
    let mut rhythm: BTreeMap<u64, RhythmEvidence> = BTreeMap::new();
    for (index, record) in reader.records().enumerate() {
        let record = record.map_err(read_error)?;
        let line = index + 2;
        let field = |column: usize| record.get(column).unwrap_or_default();
        let id: u64 = parse_field(field(ID_COLUMN), path, line)?;
        let is_correct = parse_field::<u8>(field(IS_CORRECT_COLUMN), path, line)? == 1;
        let evidence = RhythmEvidence {
            periodicity: parse_field(field(PERIODICITY_COLUMN), path, line)?,
            loop_weight: parse_field(field(LOOP_WEIGHT_COLUMN), path, line)?,
        };
        rhythm.insert(id, evidence);
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
        rhythm,
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
    /// Where to write the weights as `rhythm/octave_weights.rs`; printed when `None`.
    pub rust_out: Option<&'a Path>,
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
    calibration: &'a Calibration,
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

    let (train_sets, skipped_train) = keep_fittable(train.sets.clone());
    let (val_sets, skipped_val) = keep_fittable(val.sets.clone());
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

    let train_samples = calibration_samples(&train, model);
    let val_samples = calibration_samples(&val, model);
    let calibration = fit::fit_calibration(&train_samples, CALIBRATION_L2)?;
    println!("confidence calibration: bias {:.4}", calibration.bias);
    for (name, weight) in CONFIDENCE_INPUT_NAMES.iter().zip(&calibration.weights) {
        println!("  {name:<20} {weight:>10.4}");
    }
    print_reliability("val", &val_samples, &calibration);

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
        calibration: &calibration,
    };
    let file = File::create(args.out)
        .map_err(|error| format!("cannot create {}: {error}", args.out.display()))?;
    serde_json::to_writer_pretty(BufWriter::new(file), &weights_file)
        .map_err(|error| format!("cannot write {}: {error}", args.out.display()))?;
    println!("wrote {}", args.out.display());
    let module = weights_module(&train.feature_names, model, &calibration);
    match args.rust_out {
        Some(path) => {
            std::fs::write(path, module)
                .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
            println!("wrote {}", path.display());
        }
        None => println!("\n{module}"),
    }
    Ok(())
}

/// The confidence inputs `TempoDetector` would see for each sound it gives a tempo, with whether
/// `model`'s pick is right. Sounds without rhythm evidence get no tempo, so they're left out.
fn calibration_samples(table: &FeatureTable, model: &OctaveModel) -> Vec<CalibrationSample> {
    table
        .sets
        .iter()
        .filter_map(|set| {
            let evidence = table.rhythm.get(&set.id)?;
            let has_rhythm = evidence.periodicity > 0.0 || evidence.loop_weight > 0.0;
            let winner = model.pick(set).filter(|_| has_rhythm)?;
            let probability = model.probabilities(set)[winner];
            Some(CalibrationSample {
                inputs: confidence_inputs(probability as f32)
                    .map(f64::from)
                    .to_vec(),
                is_correct: set.is_correct[winner],
            })
        })
        .collect()
}

/// Accuracy per confidence bin, and the count-weighted gap between confidence and accuracy.
fn print_reliability(name: &str, samples: &[CalibrationSample], calibration: &Calibration) {
    let mut bins = [(0usize, 0.0f64, 0usize); RELIABILITY_BINS];
    for sample in samples {
        let confidence = calibration.probability(&sample.inputs);
        let bin = ((confidence * RELIABILITY_BINS as f64) as usize).min(RELIABILITY_BINS - 1);
        bins[bin].0 += 1;
        bins[bin].1 += confidence;
        bins[bin].2 += usize::from(sample.is_correct);
    }
    println!("{name} reliability, calibrated confidence vs Acc1:");
    let mut gap = 0.0;
    for (index, &(count, confidence_sum, correct)) in bins.iter().enumerate() {
        if count == 0 {
            continue;
        }
        let mean_confidence = confidence_sum / count as f64;
        let accuracy = correct as f64 / count as f64;
        gap += (mean_confidence - accuracy).abs() * count as f64;
        println!(
            "  {:.1}-{:.1}: {count:>5} sounds, mean confidence {:.1}%, accuracy {:.1}%",
            index as f64 / RELIABILITY_BINS as f64,
            (index + 1) as f64 / RELIABILITY_BINS as f64,
            mean_confidence * 100.0,
            accuracy * 100.0
        );
    }
    let total = samples.len().max(1) as f64;
    println!("  expected calibration error {:.1}%", gap / total * 100.0);
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

/// The fitted model as the contents of `hunch-core`'s `rhythm/octave_weights.rs`.
fn weights_module(names: &[String], model: &OctaveModel, calibration: &Calibration) -> String {
    let feature_names: Vec<&str> = names.iter().map(String::as_str).collect();
    let array = |name: &str, values: &[f64], names: &[&str]| {
        let literals: Vec<String> = values.iter().map(|v| format!("{:?},", *v as f32)).collect();
        let width = literals.iter().map(String::len).max().unwrap_or(0);
        let lines: String = literals
            .iter()
            .zip(names)
            .map(|(literal, feature)| format!("    {literal:<width$} // {feature}\n"))
            .collect();
        format!("pub const {name}: [f32; {}] = [\n{lines}];\n", values.len())
    };
    format!(
        "{WEIGHTS_MODULE_HEADER}\n{}\n{}\n{}\npub const BIAS: f32 = 0.0;\n\n{}\npub const CONFIDENCE_BIAS: f32 = {:?};\n",
        array("FEATURE_MEANS", &model.mean, &feature_names),
        array("FEATURE_STDS", &model.std, &feature_names),
        array("WEIGHTS", &model.weights, &feature_names),
        array(
            "CONFIDENCE_WEIGHTS",
            &calibration.weights,
            &CONFIDENCE_INPUT_NAMES
        ),
        calibration.bias as f32
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
            "id,split,candidate_bpm,label_bpm,expert_bpm,is_correct,hip_hop,periodicity,loop_weight,a,b\n\
             1,train,70,140,,0,1,0.5,0,0.5,1\n\
             1,train,140,140,140,1,1,0.5,0,0.25,2\n\
             2,train,90,90,,1,0,0.2,1,1,3\n",
        );
        let table = read_features_csv(&path).expect("read");
        std::fs::remove_file(&path).ok();

        let evidence = RhythmEvidence {
            periodicity: 0.2,
            loop_weight: 1.0,
        };
        assert_eq!(table.rhythm.get(&2), Some(&evidence));
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
            "id,split,candidate_bpm,label_bpm,expert_bpm,is_correct,hip_hop,periodicity,loop_weight,a\n\
             1,train,70,140,,0,1,0.5,0,oops\n",
        );
        let error = read_features_csv(&path).err().expect("error");
        std::fs::remove_file(&path).ok();
        assert!(error.contains(":2:") && error.contains("oops"), "{error}");
    }
}
