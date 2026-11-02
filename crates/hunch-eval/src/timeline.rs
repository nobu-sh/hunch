//! Dumps every estimate the analyzer emits, plus the key detector's inputs, for one split. Used to
//! study how fast and how steadily estimates settle, and to fit lock and decision rules offline.

use std::fs;
use std::path::{Path, PathBuf};

use clap::Args;
use hunch_core::tonal::KeyDetector;
use hunch_core::{Analyzer, Estimate, Key};

use crate::fsl10k::{Dataset, ExpertTempo, KeyLabel};
use crate::key_corpus::{self, Corpus};
use crate::keydata::{BEATPORT_DIR, KeyDataset, Track};
use crate::resample::{self, ANALYSIS_SAMPLE_RATE};
use crate::{DEFAULT_DATA_DIR, SplitChoice, decode, run};

/// FSL10K loops are repeated to this length; tracks are cut to the listening window.
const LISTEN_SECONDS: f32 = 30.0;
const BEATPORT_META_DIR: &str = "meta";
const PITCH_CLASSES: usize = 12;

#[derive(Args)]
pub struct TimelineArgs {
    /// Folder holding `FSL10K/`, `fsl10k-annotations/` and `beatport-key/`.
    #[arg(long, default_value = DEFAULT_DATA_DIR)]
    data: PathBuf,

    /// Split of FSL10K and Beatport to dump.
    #[arg(long, value_enum, default_value = "val")]
    split: SplitChoice,

    /// Where to write the CSV.
    #[arg(long)]
    out: PathBuf,
}

/// One sound with whatever labels it has.
struct Item {
    corpus: Corpus,
    id: u64,
    is_hip_hop: bool,
    audio_path: PathBuf,
    bpm: Option<f32>,
    expert_bpm: Option<f32>,
    keys: Vec<Key>,
    is_atonal: bool,
}

/// What the analyzer and the key detector reported at one estimate.
struct Snapshot {
    estimate: Estimate,
    presence: f32,
    tonalness: f32,
    clarity: f32,
    separation: f32,
    chroma: [f32; PITCH_CLASSES],
    bass_chroma: [f32; PITCH_CLASSES],
}

struct Timeline {
    source_seconds: f32,
    snapshots: Vec<Snapshot>,
}

pub fn run(args: &TimelineArgs) -> Result<(), String> {
    let mut items = fsl10k_items(&args.data, args.split)?;
    items.extend(beatport_items(&args.data, args.split)?);
    eprintln!("{} sounds", items.len());

    let timelines = run::par_map(&items, analyze);
    let failures = timelines
        .iter()
        .filter(|timeline| timeline.is_err())
        .count();
    if failures > 0 {
        eprintln!("{failures} sounds failed to decode and were skipped");
    }
    write_csv(&args.out, &items, &timelines)
}

fn fsl10k_items(data_dir: &Path, split: SplitChoice) -> Result<Vec<Item>, String> {
    let sounds = Dataset::load(data_dir)?.sounds;
    Ok(sounds
        .into_iter()
        .filter(|sound| split.includes(sound.split))
        .map(|sound| {
            let expert_bpm = match sound.labels.tempo_expert {
                ExpertTempo::Bpm(bpm) => Some(bpm),
                ExpertTempo::Unlabeled | ExpertTempo::Ambiguous => None,
            };
            let keys = match sound.labels.key {
                KeyLabel::Key(key) => vec![key],
                _ => Vec::new(),
            };
            Item {
                corpus: Corpus::Fsl10k,
                id: sound.id,
                is_hip_hop: sound.is_hip_hop,
                audio_path: sound.audio_path,
                bpm: sound.labels.tempo_creator,
                expert_bpm,
                keys,
                is_atonal: sound.labels.key == KeyLabel::Atonal,
            }
        })
        .collect())
}

fn beatport_items(data_dir: &Path, split: SplitChoice) -> Result<Vec<Item>, String> {
    let meta_dir = data_dir.join(BEATPORT_DIR).join(BEATPORT_META_DIR);
    let tracks = KeyDataset::load_beatport(data_dir)?.tracks;
    Ok(tracks
        .into_iter()
        .filter(|track| split.includes(track.split))
        .map(|track| Item {
            corpus: Corpus::Beatport,
            id: track.id,
            is_hip_hop: false,
            bpm: store_bpm(&meta_dir, &track),
            expert_bpm: None,
            keys: track.label.clone().unwrap_or_default(),
            is_atonal: false,
            audio_path: track.audio_path,
        })
        .collect())
}

/// The BPM Beatport lists for `track`, from its `meta/<audio stem>.json`. About half have one.
fn store_bpm(meta_dir: &Path, track: &Track) -> Option<f32> {
    let stem = track.audio_path.file_stem()?.to_str()?;
    let text = fs::read_to_string(meta_dir.join(format!("{stem}.json"))).ok()?;
    let meta: serde_json::Value = serde_json::from_str(&text).ok()?;
    Some(meta.get("bpm")?.as_f64()? as f32)
}

fn analyze(item: &Item) -> Result<Timeline, String> {
    let audio = decode::decode_mono(&item.audio_path)?;
    let samples = resample::to_analysis_rate(&audio)?;
    let source_seconds = samples.len() as f32 / ANALYSIS_SAMPLE_RATE as f32;
    let samples = match item.corpus {
        Corpus::Fsl10k => {
            let length = (LISTEN_SECONDS * ANALYSIS_SAMPLE_RATE as f32) as usize;
            resample::repeat_to_length(samples, length)
        }
        Corpus::Beatport | Corpus::Giantsteps => key_corpus::listening_window(samples),
    };

    // The analyzer owns its key detector privately, so a twin one exposes the gate and chroma.
    let mut analyzer = Analyzer::new(ANALYSIS_SAMPLE_RATE);
    let mut key_detector = KeyDetector::new(ANALYSIS_SAMPLE_RATE);
    let mut snapshots = Vec::new();
    for chunk in samples.chunks(run::chunk_len()) {
        key_detector.push(chunk);
        let Some(estimate) = analyzer.push(chunk) else {
            continue;
        };
        let key = key_detector.estimate();
        snapshots.push(Snapshot {
            estimate,
            presence: key.presence,
            tonalness: key.tonalness,
            clarity: key.clarity,
            separation: key.separation,
            chroma: key_detector.chroma(),
            bass_chroma: key_detector.bass_chroma(),
        });
    }
    Ok(Timeline {
        source_seconds,
        snapshots,
    })
}

fn write_csv(
    path: &Path,
    items: &[Item],
    timelines: &[Result<Timeline, String>],
) -> Result<(), String> {
    let mut writer = csv::Writer::from_path(path)
        .map_err(|error| format!("cannot create {}: {error}", path.display()))?;
    let write_error = |error: csv::Error| format!("cannot write {}: {error}", path.display());

    let mut header: Vec<String> = [
        "corpus",
        "id",
        "hip_hop",
        "source_seconds",
        "true_bpm",
        "expert_bpm",
        "true_keys",
        "atonal",
        "t",
        "key",
        "key_conf",
        "bpm",
        "bpm_conf",
        "bpm_alt",
        "presence",
        "tonalness",
        "clarity",
        "separation",
    ]
    .map(String::from)
    .to_vec();
    header.extend((0..PITCH_CLASSES).map(|pitch| format!("c{pitch}")));
    header.extend((0..PITCH_CLASSES).map(|pitch| format!("b{pitch}")));
    writer.write_record(&header).map_err(write_error)?;

    for (item, timeline) in items.iter().zip(timelines) {
        let Ok(timeline) = timeline else {
            continue;
        };
        let keys: Vec<String> = item.keys.iter().map(Key::to_string).collect();
        for snapshot in &timeline.snapshots {
            let estimate = &snapshot.estimate;
            let mut record = vec![
                item.corpus.to_string(),
                item.id.to_string(),
                item.is_hip_hop.to_string(),
                format!("{:.3}", timeline.source_seconds),
                optional(item.bpm),
                optional(item.expert_bpm),
                keys.join(";"),
                item.is_atonal.to_string(),
                format!("{:.2}", estimate.seconds_analyzed),
                estimate.key.map(|key| key.to_string()).unwrap_or_default(),
                format!("{:.4}", estimate.key_confidence),
                optional(estimate.bpm),
                format!("{:.4}", estimate.bpm_confidence),
                optional(estimate.bpm_alternate),
                format!("{:.4}", snapshot.presence),
                format!("{:.4}", snapshot.tonalness),
                format!("{:.4}", snapshot.clarity),
                format!("{:.4}", snapshot.separation),
            ];
            record.extend(snapshot.chroma.iter().map(|value| format!("{value:.4}")));
            record.extend(
                snapshot
                    .bass_chroma
                    .iter()
                    .map(|value| format!("{value:.4}")),
            );
            writer.write_record(&record).map_err(write_error)?;
        }
    }
    writer
        .flush()
        .map_err(|error| format!("cannot write {}: {error}", path.display()))
}

fn optional(value: Option<f32>) -> String {
    value.map(|value| format!("{value:.3}")).unwrap_or_default()
}
