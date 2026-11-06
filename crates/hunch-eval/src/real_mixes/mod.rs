//! `real-mixes`: scores session windows on recorded mixes against hand-marked changes.
//!
//! Each `<name>.wav` in the folder (recorded with `hunch-capture listen --record`) is paired with
//! `<name>.csv` beside it; see [`marks`] for its format.

mod marks;
mod score;
mod session;

use std::path::{Path, PathBuf};
use std::time::Instant;

use clap::Args;
use walkdir::WalkDir;

use crate::key_sweep::pct;
use crate::metrics;
use crate::run;
use marks::Mark;
use score::{MarkKind, MarkOutcome, READOUT_CHECKPOINTS, Score, ScoreInput, Tally};

const WAV_EXTENSION: &str = "wav";
const MARKS_EXTENSION: &str = "csv";
const SECONDS_PER_HOUR: f32 = 3600.0;
const NAME_WIDTH: usize = 24;

#[derive(Args)]
pub struct RealMixesArgs {
    /// Folder searched recursively for `<name>.wav` recordings with `<name>.csv` marks beside them.
    #[arg(long)]
    dir: PathBuf,

    /// Also list every mark after the first and the boundary matched to it.
    #[arg(long)]
    details: bool,
}

struct Recording {
    name: String,
    wav: PathBuf,
    marks: PathBuf,
}

struct Scored {
    score: Score,
    outcomes: Vec<MarkOutcome>,
}

pub fn run(args: &RealMixesArgs) -> Result<(), String> {
    let started = Instant::now();
    let recordings = find_recordings(&args.dir);
    if recordings.is_empty() {
        return Err(format!(
            "no .wav recordings with .csv marks beside them in {}",
            args.dir.display()
        ));
    }
    let results = run::par_map(&recordings, score_recording);

    print_header();
    let mut total = Score::default();
    for (recording, result) in recordings.iter().zip(&results) {
        match result {
            Ok(scored) => {
                print_row(&recording.name, &scored.score);
                total.merge(&scored.score);
            }
            Err(error) => eprintln!("warning: skipping {}: {error}", recording.name),
        }
    }
    print_row("total", &total);

    if args.details {
        for (recording, result) in recordings.iter().zip(&results) {
            if let Ok(scored) = result {
                print_details(&recording.name, &scored.outcomes);
            }
        }
    }
    println!("\nwall clock {:.1}s", started.elapsed().as_secs_f32());
    Ok(())
}

fn find_recordings(dir: &Path) -> Vec<Recording> {
    let mut recordings: Vec<Recording> = WalkDir::new(dir)
        .into_iter()
        .filter_map(|entry| match entry {
            Ok(entry) => Some(entry.into_path()),
            Err(error) => {
                eprintln!("warning: {error}");
                None
            }
        })
        .filter(|path| has_extension(path, WAV_EXTENSION))
        .filter_map(|wav| {
            let marks = wav.with_extension(MARKS_EXTENSION);
            if !marks.is_file() {
                eprintln!("warning: {} has no {}", wav.display(), marks.display());
                return None;
            }
            let name = wav.strip_prefix(dir).unwrap_or(&wav).display().to_string();
            Some(Recording { name, wav, marks })
        })
        .collect();
    recordings.sort_by(|a, b| a.name.cmp(&b.name));
    recordings
}

fn has_extension(path: &Path, wanted: &str) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case(wanted))
}

fn score_recording(recording: &Recording) -> Result<Scored, String> {
    let marks: Vec<Mark> = marks::read_marks(&recording.marks)?;
    let session = session::run_session(&recording.wav)?;
    let (score, outcomes) = score::score(&ScoreInput {
        marks: &marks,
        events: &session.events,
        audio_seconds: session.audio_seconds,
    });
    Ok(Scored { score, outcomes })
}

fn print_header() {
    println!(
        "{:<NAME_WIDTH$} {:>6} {:>5} {:>5} {:>6} {:>7} {:>7} {:>7} {:>9} | {:>18} | {:>18}",
        "file",
        "hours",
        "chg",
        "found",
        "recall",
        "delay50",
        "cperr50",
        "false/h",
        "unch spl",
        "key % +10/+30/end",
        "bpm % +10/+30/end"
    );
}

fn print_row(name: &str, score: &Score) {
    println!(
        "{:<NAME_WIDTH$} {:>6.2} {:>5} {:>5} {:>6} {:>7} {:>7} {:>7} {:>9} | {:>18} | {:>18}",
        name,
        score.scored_seconds / SECONDS_PER_HOUR,
        score.changes,
        score.found(),
        pct(metrics::ratio(score.found(), score.changes)),
        seconds(median(&score.delays)),
        seconds(median(&score.change_point_errors)),
        score
            .false_splits_per_hour()
            .map_or("-".to_string(), |rate| format!("{rate:.2}")),
        format!("{}/{}", score.unchanged_marks_split, score.unchanged_marks),
        readouts(&score.key_readouts),
        readouts(&score.bpm_readouts),
    );
}

fn readouts(tallies: &[Tally; READOUT_CHECKPOINTS]) -> String {
    tallies
        .iter()
        .map(|tally| pct(tally.accuracy()))
        .collect::<Vec<_>>()
        .join("/")
}

fn seconds(value: Option<f32>) -> String {
    value.map_or("-".to_string(), |value| format!("{value:+.1}"))
}

fn median(values: &[f32]) -> Option<f32> {
    let mut sorted = values.to_vec();
    sorted.sort_by(f32::total_cmp);
    metrics::median(&sorted)
}

fn print_details(name: &str, outcomes: &[MarkOutcome]) {
    println!("\n{name}");
    for outcome in outcomes {
        let kind = match outcome.kind {
            MarkKind::First => continue,
            MarkKind::Change { key, bpm } => match (key, bpm) {
                (true, true) => "key+bpm change",
                (true, false) => "key change",
                _ => "bpm change",
            },
            MarkKind::Unchanged => "unchanged",
        };
        let result = match (outcome.boundary, outcome.kind) {
            (Some(boundary), _) => format!(
                "split detected {:+.1} s, placed {:+.1} s",
                boundary.detected_at - outcome.start_seconds,
                boundary.change_point - outcome.start_seconds
            ),
            (None, MarkKind::Change { .. }) => "missed".to_string(),
            (None, _) => "no split".to_string(),
        };
        println!(
            "  {:>9.1} s  {kind:<15} {result:<38} {}",
            outcome.start_seconds, outcome.note
        );
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::TAU;

    use super::*;
    use crate::resample::ANALYSIS_SAMPLE_RATE;

    const TONE_HZ: f32 = 220.0;
    const TONE_AMPLITUDE: f32 = 0.3;
    const SOUND_SECONDS: f32 = 20.0;
    const GAP_SECONDS: f32 = 3.0;

    /// Two stretches of tone with a silent gap, so today's silence splitting finds the change.
    fn write_two_part_wav(path: &Path) {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: ANALYSIS_SAMPLE_RATE,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(path, spec).expect("create wav");
        let total_seconds = 2.0 * SOUND_SECONDS + GAP_SECONDS;
        let total_samples = (total_seconds * ANALYSIS_SAMPLE_RATE as f32) as u32;
        for n in 0..total_samples {
            let t = n as f32 / ANALYSIS_SAMPLE_RATE as f32;
            let is_gap = (SOUND_SECONDS..SOUND_SECONDS + GAP_SECONDS).contains(&t);
            let sample = if is_gap {
                0.0
            } else {
                (t * TONE_HZ * TAU).sin() * TONE_AMPLITUDE
            };
            writer
                .write_sample((sample * f32::from(i16::MAX)) as i16)
                .expect("write sample");
        }
        writer.finalize().expect("finalize wav");
    }

    #[test]
    fn finds_a_change_at_a_silent_gap_in_a_recording() {
        let dir = std::env::temp_dir().join("hunch-eval-real-mixes-test");
        std::fs::create_dir_all(&dir).expect("create dir");
        let wav = dir.join("mix.wav");
        write_two_part_wav(&wav);
        let change_at = SOUND_SECONDS + GAP_SECONDS;
        let marks = format!("start_seconds,key,bpm,note\n0,A minor,120,\n{change_at},D minor,,\n");
        std::fs::write(dir.join("mix.csv"), marks).expect("write marks");

        let recordings = find_recordings(&dir);
        let scored = score_recording(&recordings[0]);
        std::fs::remove_dir_all(&dir).ok();

        let score = scored.expect("scored").score;
        assert_eq!(recordings.len(), 1);
        assert_eq!(score.changes, 1);
        assert_eq!(score.found(), 1);
        assert_eq!(score.false_splits, 0);
        assert!(score.change_point_errors[0].abs() < 1.0, "{score:?}");
    }
}
