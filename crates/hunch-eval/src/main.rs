//! Offline harness: decode audio files and run them through `hunch-core`.

mod decode;

use std::path::{Path, PathBuf};

use clap::Parser;
use hunch_core::{Analyzer, Estimate};
use walkdir::WalkDir;

use crate::decode::MonoAudio;

const AUDIO_EXTENSIONS: [&str; 6] = ["wav", "mp3", "flac", "ogg", "aiff", "aif"];
/// Matches the chunk size the extension's AudioWorklet posts.
const CHUNK_SECONDS: f32 = 0.1;

#[derive(Parser)]
#[command(version, about = "Run hunch-core over a directory of audio files")]
struct Args {
    /// Directory to search recursively for audio files.
    dir: PathBuf,

    /// Stop after this many files.
    #[arg(long)]
    limit: Option<usize>,
}

fn main() {
    let args = Args::parse();
    let files = find_audio_files(&args.dir);
    let limit = args.limit.unwrap_or(usize::MAX);

    for path in files.iter().take(limit) {
        match decode::decode_mono(path) {
            Ok(audio) => print_result(path, &audio),
            Err(error) => eprintln!("warning: skipping {}: {error}", path.display()),
        }
    }
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
