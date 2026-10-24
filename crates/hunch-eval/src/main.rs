//! Offline harness: decode audio files and run them through `hunch-core`.

mod decode;
mod fit;
mod fsl10k;
mod key_corpus;
mod key_sweep;
mod keydata;
mod metrics;
mod octave;
mod profile_fit;
mod report;
mod resample;
mod run;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use clap::{Args, Parser, Subcommand, ValueEnum};
use hunch_core::rhythm::OCTAVE_FEATURE_NAMES;
use hunch_core::{Analyzer, Estimate};
use walkdir::WalkDir;

use crate::decode::MonoAudio;
use crate::fsl10k::{Dataset, Sound, Split};
use crate::octave::{FitOctaveArgs, SoundCandidates};
use crate::report::{Evaluated, SubsetReport, Summary};
use crate::run::CHUNK_SECONDS;

const AUDIO_EXTENSIONS: [&str; 6] = ["wav", "mp3", "flac", "ogg", "aiff", "aif"];
pub(crate) const DEFAULT_DATA_DIR: &str = r"G:\hunch\data";
const DEFAULT_L2: f64 = 1e-2;
pub(crate) const MAX_LISTED_FAILURES: usize = 20;

#[derive(Parser)]
#[command(version, about = "Evaluate hunch-core on audio files and datasets")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run hunch-core over a directory of audio files and print each estimate.
    Scan(ScanArgs),
    /// Score hunch-core against FSL10K creator and expert labels.
    Fsl10k(Fsl10kArgs),
    /// Dump TempoDetector octave candidates and their features, one CSV row per candidate.
    OctaveFeatures(OctaveFeaturesArgs),
    /// Fit the tempo octave decision weights from octave-features CSVs.
    FitOctave(FitOctaveCliArgs),
    /// Score a grid of KeyDetector configs on FSL10K keyed and atonal sounds.
    KeySweep(Box<key_sweep::KeySweepArgs>),
}

#[derive(Args)]
struct ScanArgs {
    /// Directory to search recursively for audio files.
    dir: PathBuf,

    /// Stop after this many files.
    #[arg(long)]
    limit: Option<usize>,
}

#[derive(Clone, Copy, ValueEnum)]
enum SplitChoice {
    Train,
    Val,
    Test,
    All,
}

impl SplitChoice {
    fn includes(self, split: Split) -> bool {
        match self {
            SplitChoice::Train => split == Split::Train,
            SplitChoice::Val => split == Split::Val,
            SplitChoice::Test => split == Split::Test,
            SplitChoice::All => true,
        }
    }

    fn name(self) -> &'static str {
        match self {
            SplitChoice::Train => "train",
            SplitChoice::Val => "val",
            SplitChoice::Test => "test",
            SplitChoice::All => "all",
        }
    }
}

#[derive(Args)]
struct DatasetArgs {
    /// Folder holding `FSL10K/` and `fsl10k-annotations/`.
    #[arg(long, default_value = DEFAULT_DATA_DIR)]
    data: PathBuf,

    #[arg(long, value_enum, default_value = "val")]
    split: SplitChoice,

    /// Stop after this many sounds (lowest ids first).
    #[arg(long)]
    limit: Option<usize>,

    /// Only these Freesound ids, comma-separated.
    #[arg(long, value_delimiter = ',')]
    ids: Vec<u64>,

    /// Repeat each loop until it is at least this long.
    #[arg(long, default_value_t = 0.0)]
    min_seconds: f32,
}

#[derive(Args)]
struct Fsl10kArgs {
    #[command(flatten)]
    dataset: DatasetArgs,

    /// Write one row per sound here.
    #[arg(long)]
    csv: Option<PathBuf>,

    /// Write the metric summary here.
    #[arg(long)]
    json: Option<PathBuf>,
}

#[derive(Args)]
struct OctaveFeaturesArgs {
    #[command(flatten)]
    dataset: DatasetArgs,

    #[arg(long)]
    out: PathBuf,
}

#[derive(Args)]
struct FitOctaveCliArgs {
    /// octave-features CSV to fit on.
    #[arg(long)]
    train: PathBuf,

    /// octave-features CSV to report accuracy on.
    #[arg(long)]
    val: PathBuf,

    /// Where to write the fitted weights JSON.
    #[arg(long)]
    out: PathBuf,

    /// Also write the weights as hunch-core's `rhythm/octave_weights.rs` here.
    #[arg(long)]
    rust_out: Option<PathBuf>,

    /// L2 regularization strength.
    #[arg(long, default_value_t = DEFAULT_L2)]
    l2: f64,
}

fn main() -> ExitCode {
    let result = match Cli::parse().command {
        Command::Scan(args) => {
            scan(&args);
            Ok(())
        }
        Command::Fsl10k(args) => evaluate_fsl10k(&args),
        Command::OctaveFeatures(args) => dump_octave_features(&args),
        Command::FitOctave(args) => octave::fit_octave(&FitOctaveArgs {
            train: &args.train,
            val: &args.val,
            out: &args.out,
            rust_out: args.rust_out.as_deref(),
            l2: args.l2,
        }),
        Command::KeySweep(args) => key_sweep::run(&args),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn scan(args: &ScanArgs) {
    let files = find_audio_files(&args.dir);
    let limit = args.limit.unwrap_or(usize::MAX);

    for path in files.iter().take(limit) {
        match decode::decode_mono(path) {
            Ok(audio) => print_result(path, &audio),
            Err(error) => eprintln!("warning: skipping {}: {error}", path.display()),
        }
    }
}

/// Loads FSL10K, prints its label counts per split, and returns the sounds `args` selects.
fn load_selection(args: &DatasetArgs) -> Result<(Dataset, Vec<usize>), String> {
    let started = Instant::now();
    let dataset = Dataset::load(&args.data)?;
    println!(
        "loaded {} FSL10K sounds with audio in {:.1}s ({} metadata ids without audio skipped)",
        dataset.sounds.len(),
        started.elapsed().as_secs_f32(),
        dataset.missing_audio.len()
    );
    report::print_label_stats(&report::label_stats_by_split(&dataset.sounds));

    let missing_ids: Vec<u64> = args
        .ids
        .iter()
        .copied()
        .filter(|id| !dataset.sounds.iter().any(|sound| sound.id == *id))
        .collect();
    if !missing_ids.is_empty() {
        eprintln!("warning: ids not in the dataset (or without audio): {missing_ids:?}");
    }
    let selected: Vec<usize> = dataset
        .sounds
        .iter()
        .enumerate()
        .filter(|(_, sound)| args.split.includes(sound.split))
        .filter(|(_, sound)| args.ids.is_empty() || args.ids.contains(&sound.id))
        .map(|(index, _)| index)
        .take(args.limit.unwrap_or(usize::MAX))
        .collect();
    println!(
        "selected {} sounds from split {}",
        selected.len(),
        args.split.name()
    );
    Ok((dataset, selected))
}

/// Counts failures by the part of the message before the first `:`, and lists the first few.
fn summarize_failures(failures: &[(&Sound, String)]) -> BTreeMap<String, usize> {
    let mut reasons: BTreeMap<String, usize> = BTreeMap::new();
    for (sound, error) in failures {
        let reason = error.split(':').next().unwrap_or(error).to_string();
        *reasons.entry(reason).or_default() += 1;
        if reasons.values().sum::<usize>() <= MAX_LISTED_FAILURES {
            eprintln!(
                "warning: skipping {} ({}): {error}",
                sound.id,
                sound.audio_path.display()
            );
        }
    }
    reasons
}

fn evaluate_fsl10k(args: &Fsl10kArgs) -> Result<(), String> {
    let started = Instant::now();
    let (dataset, selected) = load_selection(&args.dataset)?;
    let sounds: Vec<&Sound> = selected.iter().map(|&i| &dataset.sounds[i]).collect();
    let min_seconds = args.dataset.min_seconds;

    let outcomes = run::par_map_sounds(&sounds, |sound| {
        let samples = run::prepare_audio(&sound.audio_path, min_seconds)?;
        Ok(run::analyze(&samples))
    });
    let mut results = Vec::new();
    let mut failures = Vec::new();
    for (sound, outcome) in sounds.iter().copied().zip(outcomes) {
        match outcome {
            Ok(run) => results.push(Evaluated { sound, run }),
            Err(error) => failures.push((sound, error)),
        }
    }
    let skipped = summarize_failures(&failures);
    println!(
        "evaluated {} sounds, skipped {} {skipped:?}",
        results.len(),
        failures.len()
    );

    let all: Vec<&Evaluated> = results.iter().collect();
    let hip_hop: Vec<&Evaluated> = results.iter().filter(|r| r.sound.is_hip_hop).collect();
    let subsets = vec![
        SubsetReport::build("all selected", &all),
        SubsetReport::build("hip hop", &hip_hop),
    ];
    for subset in &subsets {
        subset.print();
    }

    if let Some(path) = &args.csv {
        report::write_csv(path, &results)?;
        println!("\nwrote {}", path.display());
    }
    if let Some(path) = &args.json {
        let labels = report::label_stats_by_split(&dataset.sounds);
        let summary = Summary {
            split: args.dataset.split.name().to_string(),
            min_seconds,
            selected: sounds.len(),
            evaluated: results.len(),
            skipped,
            labels: &labels,
            subsets,
        };
        report::write_json(path, &summary)?;
        println!("wrote {}", path.display());
    }
    println!("\nwall clock {:.1}s", started.elapsed().as_secs_f32());
    Ok(())
}

fn dump_octave_features(args: &OctaveFeaturesArgs) -> Result<(), String> {
    let started = Instant::now();
    let (dataset, selected) = load_selection(&args.dataset)?;
    let sounds: Vec<&Sound> = selected
        .iter()
        .map(|&i| &dataset.sounds[i])
        .filter(|sound| sound.labels.tempo_creator.is_some())
        .collect();
    println!(
        "{} selected sounds have a creator tempo label",
        sounds.len()
    );
    if OCTAVE_FEATURE_NAMES.is_empty() {
        eprintln!("warning: OCTAVE_FEATURE_NAMES is empty, so rows will carry no features");
    }

    let min_seconds = args.dataset.min_seconds;
    let outcomes = run::par_map_sounds(&sounds, |sound| {
        let samples = run::prepare_audio(&sound.audio_path, min_seconds)?;
        Ok(octave::analysis_for(&samples))
    });
    let mut entries = Vec::new();
    let mut failures = Vec::new();
    for (sound, outcome) in sounds.iter().copied().zip(outcomes) {
        match outcome {
            Ok(analysis) => entries.push(SoundCandidates { sound, analysis }),
            Err(error) => failures.push((sound, error)),
        }
    }
    let skipped = summarize_failures(&failures);
    octave::check_feature_lengths(&entries)?;

    let without_candidates = entries
        .iter()
        .filter(|e| e.analysis.candidates.is_empty())
        .count();
    let rows = octave::write_features_csv(&args.out, &entries)?;
    println!(
        "analyzed {} sounds, skipped {} {skipped:?}; {without_candidates} had no candidates",
        entries.len(),
        failures.len()
    );
    if rows == 0 {
        eprintln!(
            "warning: TempoDetector::octave_candidates() returned nothing for every sound; {} has only a header",
            args.out.display()
        );
    }
    println!(
        "wrote {rows} candidate rows to {} in {:.1}s",
        args.out.display(),
        started.elapsed().as_secs_f32()
    );
    Ok(())
}

fn find_audio_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = WalkDir::new(dir)
        .into_iter()
        .filter_map(|entry| match entry {
            Ok(entry) => Some(entry),
            Err(error) => {
                eprintln!("warning: {error}");
                None
            }
        })
        .filter(|entry| entry.file_type().is_file() && is_audio_file(entry.path()))
        .map(walkdir::DirEntry::into_path)
        .collect();
    files.sort();
    files
}

fn is_audio_file(path: &Path) -> bool {
    let Some(extension) = path.extension().and_then(|ext| ext.to_str()) else {
        return false;
    };
    AUDIO_EXTENSIONS
        .iter()
        .any(|known| known.eq_ignore_ascii_case(extension))
}

fn analyze(audio: &MonoAudio) -> Option<Estimate> {
    let mut analyzer = Analyzer::new(audio.sample_rate);
    let chunk_len = ((audio.sample_rate as f32 * CHUNK_SECONDS) as usize).max(1);
    let mut last_estimate = None;
    for chunk in audio.samples.chunks(chunk_len) {
        if let Some(estimate) = analyzer.push(chunk) {
            last_estimate = Some(estimate);
        }
    }
    last_estimate
}

fn print_result(path: &Path, audio: &MonoAudio) {
    let duration = audio.samples.len() as f32 / audio.sample_rate as f32;
    let estimate = match analyze(audio) {
        Some(estimate) => format_estimate(&estimate),
        None => "—".to_string(),
    };
    println!("{}\t{duration:.1}s\t{estimate}", path.display());
}

fn format_estimate(estimate: &Estimate) -> String {
    let key = match estimate.key {
        Some(key) => format!("{key} ({:.0}%)", estimate.key_confidence * 100.0),
        None => "key —".to_string(),
    };
    let bpm = match estimate.bpm {
        Some(bpm) => format!("{bpm:.1} BPM ({:.0}%)", estimate.bpm_confidence * 100.0),
        None => "BPM —".to_string(),
    };
    format!("{key}, {bpm} after {:.1}s", estimate.seconds_analyzed)
}
