//! Feeds dataset sounds through `hunch-core` the way the extension does.

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use hunch_core::{Analyzer, Estimate};
use rayon::prelude::*;

use crate::decode;
use crate::fsl10k::Sound;
use crate::resample::{self, ANALYSIS_SAMPLE_RATE};

/// Matches the chunk size the extension's AudioWorklet posts.
pub const CHUNK_SECONDS: f32 = 0.1;
const PROGRESS_EVERY: usize = 500;

/// Decodes `path` to mono at `ANALYSIS_SAMPLE_RATE`, looped to at least `min_seconds`.
pub fn prepare_audio(path: &Path, min_seconds: f32) -> Result<Vec<f32>, String> {
    let audio = decode::decode_mono(path)?;
    let samples = resample::to_analysis_rate(&audio)?;
    if samples.is_empty() {
        return Err("no samples".to_string());
    }
    let min_len = (min_seconds.max(0.0) * ANALYSIS_SAMPLE_RATE as f32).ceil() as usize;
    Ok(resample::repeat_to_length(samples, min_len))
}

/// Samples per pushed chunk at the analysis rate.
pub fn chunk_len() -> usize {
    (ANALYSIS_SAMPLE_RATE as f32 * CHUNK_SECONDS) as usize
}

/// What the analyzer reported for one sound.
pub struct SoundRun {
    /// Every estimate `push` emitted, about one per second.
    pub timeline: Vec<Estimate>,
    pub last: Estimate,
}

pub fn analyze(samples: &[f32]) -> SoundRun {
    let mut analyzer = Analyzer::new(ANALYSIS_SAMPLE_RATE);
    let timeline = samples
        .chunks(chunk_len())
        .filter_map(|chunk| analyzer.push(chunk))
        .collect();
    SoundRun {
        timeline,
        last: analyzer.current(),
    }
}

/// Runs `job` over `sounds` in parallel, keeping input order and printing progress to stderr.
pub fn par_map_sounds<T: Send>(
    sounds: &[&Sound],
    job: impl Fn(&Sound) -> Result<T, String> + Sync,
) -> Vec<Result<T, String>> {
    par_map(sounds, |sound| job(sound))
}

/// Runs `job` over `items` in parallel, keeping input order and printing progress to stderr.
pub fn par_map<I: Sync, T: Send>(
    items: &[I],
    job: impl Fn(&I) -> Result<T, String> + Sync,
) -> Vec<Result<T, String>> {
    let done = AtomicUsize::new(0);
    let total = items.len();
    items
        .par_iter()
        .map(|item| {
            let result = job(item);
            let finished = done.fetch_add(1, Ordering::Relaxed) + 1;
            if finished % PROGRESS_EVERY == 0 {
                eprintln!("  {finished}/{total} sounds");
            }
            result
        })
        .collect()
}
