//! Key-labeled sounds from every dataset, in one shape: FSL10K loops (keyed and atonal) and
//! Beatport / GiantSteps+ tracks (keyed only, possibly with alternatives).

use std::fmt;
use std::path::{Path, PathBuf};

use clap::{Args, ValueEnum};
use hunch_core::Key;

use crate::fsl10k::{Dataset, KeyLabel, Split};
use crate::keydata::{self, KeyDataset};
use crate::resample::{self, ANALYSIS_SAMPLE_RATE};
use crate::{DEFAULT_DATA_DIR, SplitChoice, decode};

/// The browser listening window skips this much of a track's intro.
const WINDOW_START_SECONDS: f32 = 30.0;
const WINDOW_SECONDS: f32 = 30.0;
/// FSL10K loops are repeated to this length unless `--min-seconds` says otherwise.
const DEFAULT_MIN_SECONDS: f32 = 15.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, ValueEnum)]
pub enum Corpus {
    Fsl10k,
    Beatport,
    /// Held-out test set: score it only when the coordinator asks.
    Giantsteps,
}

impl fmt::Display for Corpus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Corpus::Fsl10k => "fsl10k",
            Corpus::Beatport => "beatport",
            Corpus::Giantsteps => "giantsteps",
        })
    }
}

/// How much of a track is analyzed. FSL10K loops are always analyzed whole (looped).
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Window {
    /// The whole excerpt (two minutes for Beatport and GiantSteps+).
    Full,
    /// 30 s starting 30 s in, like a listener in the browser past the intro.
    #[value(name = "30s")]
    ThirtySeconds,
}

impl fmt::Display for Window {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Window::Full => "full",
            Window::ThirtySeconds => "30s",
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Truth {
    /// Any of these keys counts as correct.
    Keys(Vec<Key>),
    /// Annotators agreed there is no key.
    Atonal,
}

#[derive(Debug, Clone)]
pub struct KeyItem {
    pub corpus: Corpus,
    pub id: u64,
    pub split: Split,
    pub audio_path: PathBuf,
    pub truth: Truth,
    pub is_hip_hop: bool,
}

impl KeyItem {
    pub fn labels(&self) -> Option<&[Key]> {
        match &self.truth {
            Truth::Keys(keys) => Some(keys),
            Truth::Atonal => None,
        }
    }

    pub fn is_atonal(&self) -> bool {
        self.truth == Truth::Atonal
    }
}

#[derive(Args)]
pub struct CorpusArgs {
    /// Folder holding `FSL10K/`, `fsl10k-annotations/`, `beatport-key/` and `giantsteps-key/`.
    #[arg(long, default_value = DEFAULT_DATA_DIR)]
    pub data: PathBuf,

    /// Datasets to score, comma-separated. GiantSteps+ is the held-out test set.
    #[arg(long, value_enum, value_delimiter = ',', default_values_t = [Corpus::Fsl10k, Corpus::Beatport])]
    pub datasets: Vec<Corpus>,

    /// Split for FSL10K and Beatport (GiantSteps+ is all test).
    #[arg(long, value_enum, default_value = "val")]
    pub split: SplitChoice,

    /// Repeat each FSL10K loop until it is at least this long.
    #[arg(long, default_value_t = DEFAULT_MIN_SECONDS)]
    pub min_seconds: f32,

    /// Stop after this many sounds per dataset (lowest ids first).
    #[arg(long)]
    pub limit: Option<usize>,
}

/// Loads the keyed and atonal sounds of `datasets` in `split`, printing label stats.
pub fn load_items(args: &CorpusArgs, split: SplitChoice) -> Result<Vec<KeyItem>, String> {
    let mut items = Vec::new();
    for &corpus in &args.datasets {
        let mut loaded = match corpus {
            Corpus::Fsl10k => fsl10k_items(&args.data)?,
            Corpus::Beatport => track_items(corpus, &KeyDataset::load_beatport(&args.data)?),
            Corpus::Giantsteps => track_items(corpus, &KeyDataset::load_giantsteps(&args.data)?),
        };
        let is_test_set = corpus == Corpus::Giantsteps;
        loaded.retain(|item| is_test_set || split.includes(item.split));
        loaded.truncate(args.limit.unwrap_or(usize::MAX));
        let keyed = loaded.iter().filter(|item| !item.is_atonal()).count();
        println!(
            "{corpus}: {} sounds from split {} ({keyed} keyed, {} atonal)",
            loaded.len(),
            if is_test_set { "test" } else { split.name() },
            loaded.len() - keyed
        );
        items.extend(loaded);
    }
    Ok(items)
}

fn fsl10k_items(data_dir: &Path) -> Result<Vec<KeyItem>, String> {
    let dataset = Dataset::load(data_dir)?;
    Ok(dataset
        .sounds
        .into_iter()
        .filter_map(|sound| {
            let truth = match sound.labels.key {
                KeyLabel::Key(key) => Truth::Keys(vec![key]),
                KeyLabel::Atonal => Truth::Atonal,
                KeyLabel::Unlabeled | KeyLabel::Ambiguous => return None,
            };
            Some(KeyItem {
                corpus: Corpus::Fsl10k,
                id: sound.id,
                split: sound.split,
                audio_path: sound.audio_path,
                truth,
                is_hip_hop: sound.is_hip_hop,
            })
        })
        .collect())
}

fn track_items(corpus: Corpus, dataset: &KeyDataset) -> Vec<KeyItem> {
    keydata::print_label_stats(dataset);
    dataset
        .tracks
        .iter()
        .filter_map(|track| {
            let keys = track.label.as_ref().ok()?;
            Some(KeyItem {
                corpus,
                id: track.id,
                split: track.split,
                audio_path: track.audio_path.clone(),
                truth: Truth::Keys(keys.clone()),
                is_hip_hop: false,
            })
        })
        .collect()
}

pub struct Prepared {
    /// Mono at the analysis rate.
    pub samples: Vec<f32>,
    /// Length of the decoded audio before looping or windowing.
    pub source_seconds: f32,
}

/// An FSL10K loop repeated to `min_seconds`, or the `window` of a track.
pub fn prepare(item: &KeyItem, window: Window, min_seconds: f32) -> Result<Prepared, String> {
    let audio = decode::decode_mono(&item.audio_path)?;
    let samples = resample::to_analysis_rate(&audio)?;
    if samples.is_empty() {
        return Err("no samples".to_string());
    }
    let source_seconds = samples.len() as f32 / ANALYSIS_SAMPLE_RATE as f32;
    let samples = match (item.corpus, window) {
        (Corpus::Fsl10k, _) => {
            resample::repeat_to_length(samples, seconds_to_samples(min_seconds.max(0.0)))
        }
        (_, Window::Full) => samples,
        (_, Window::ThirtySeconds) => listening_window(samples),
    };
    Ok(Prepared {
        samples,
        source_seconds,
    })
}

/// `WINDOW_SECONDS` from `WINDOW_START_SECONDS` in, or the last `WINDOW_SECONDS` of a track too
/// short for that (all of it when shorter still).
fn listening_window(mut samples: Vec<f32>) -> Vec<f32> {
    let len = seconds_to_samples(WINDOW_SECONDS);
    let start = seconds_to_samples(WINDOW_START_SECONDS).min(samples.len().saturating_sub(len));
    samples.truncate(start + len);
    samples.drain(..start);
    samples
}

fn seconds_to_samples(seconds: f32) -> usize {
    (seconds * ANALYSIS_SAMPLE_RATE as f32).ceil() as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listening_window_skips_the_intro() {
        let rate = ANALYSIS_SAMPLE_RATE as usize;
        let track: Vec<f32> = (0..120 * rate).map(|i| (i / rate) as f32).collect();
        let window = listening_window(track);
        assert_eq!(window.len(), 30 * rate);
        assert_eq!(window[0], 30.0);
        assert_eq!(window[window.len() - 1], 59.0);
    }

    #[test]
    fn listening_window_falls_back_to_the_end_of_short_tracks() {
        let rate = ANALYSIS_SAMPLE_RATE as usize;
        let track: Vec<f32> = (0..40 * rate).map(|i| (i / rate) as f32).collect();
        let window = listening_window(track);
        assert_eq!(window.len(), 30 * rate);
        assert_eq!(window[0], 10.0);

        let tiny = vec![1.0; rate];
        assert_eq!(listening_window(tiny).len(), rate);
    }
}
