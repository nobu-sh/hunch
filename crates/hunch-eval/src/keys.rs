//! `keys`: the default key detector's metrics per dataset and listening window.

use std::time::Instant;

use clap::Args;
use hunch_core::tonal::{KeyDetector, KeyDetectorConfig};

use crate::key_corpus::{self, Corpus, CorpusArgs, KeyItem, Window};
use crate::key_sweep::{self, ATONAL_WEIGHT_SHARE, Scored, pct};
use crate::metrics;
use crate::resample::ANALYSIS_SAMPLE_RATE;
use crate::run;

/// `estimate()` is timed over this many calls.
const ESTIMATE_TIMING_CALLS: u32 = 1_000;

#[derive(Args)]
pub struct KeysArgs {
    #[command(flatten)]
    corpus: CorpusArgs,

    /// Parts of each Beatport/GiantSteps+ track to analyze; FSL10K loops are scored once.
    #[arg(long, value_enum, value_delimiter = ',', default_values_t = [Window::ThirtySeconds, Window::Full])]
    windows: Vec<Window>,

    /// Only decode the selected audio and count what loads; no predictions.
    #[arg(long)]
    decode_only: bool,
}

pub fn run(args: &KeysArgs) -> Result<(), String> {
    let started = Instant::now();
    let items = key_corpus::load_items(&args.corpus, args.corpus.split)?;
    if args.decode_only {
        count_decodable(&items, &args.corpus);
        return Ok(());
    }

    println!(
        "\n{:<10} {:<5} {:>5} {:>6} {:>6} {:>6} {:>6} | {:>6} {:>6} | {:>6} {:>6} | {:>13}",
        "dataset",
        "win",
        "keyed",
        "cov",
        "exact",
        "same",
        "mirex",
        "atonal",
        "fkey",
        "ece",
        "ece-rw",
        "conf ok/wrong"
    );
    let config = [KeyDetectorConfig::default()];
    for (index, &window) in args.windows.iter().enumerate() {
        // Loops ignore the window, so only the first pass scores them.
        let selected: Vec<KeyItem> = items
            .iter()
            .filter(|item| index == 0 || item.corpus != Corpus::Fsl10k)
            .cloned()
            .collect();
        let scored = key_sweep::analyze_items(&selected, window, args.corpus.min_seconds, &config);
        for corpus in key_sweep::corpora(&scored) {
            let window_name = match corpus {
                Corpus::Fsl10k => format!("{}s", args.corpus.min_seconds),
                Corpus::Beatport | Corpus::Giantsteps => window.to_string(),
            };
            print_row(corpus, &window_name, &key_sweep::subset(&scored, corpus));
        }
    }

    if let Some(item) = items.iter().find(|item| item.corpus != Corpus::Fsl10k) {
        print_timing(item)?;
    }
    println!("\nwall clock {:.1}s", started.elapsed().as_secs_f32());
    Ok(())
}

fn print_row(corpus: Corpus, window: &str, scored: &[&Scored]) {
    // Keys are already gated by the detector's own presence threshold.
    let row = key_sweep::score_row(scored, 0, 0.0);
    let keyed_points: Vec<(f64, bool, f64)> = scored
        .iter()
        .filter_map(|entry| {
            let outcome = &entry.result.outcomes[0];
            let key = outcome.key?;
            let is_correct = metrics::is_same_scale_as_any(key, entry.labels()?);
            Some((f64::from(outcome.confidence), is_correct, 1.0))
        })
        .collect();
    let false_keys: Vec<f64> = scored
        .iter()
        .filter(|entry| entry.is_atonal())
        .filter_map(|entry| {
            let outcome = &entry.result.outcomes[0];
            outcome.key.map(|_| f64::from(outcome.confidence))
        })
        .collect();
    let reweighted = reweighted_points(&keyed_points, &false_keys);
    let mean_confidence = |wanted: bool| {
        let values: Vec<f64> = keyed_points
            .iter()
            .filter(|(_, is_correct, _)| *is_correct == wanted)
            .map(|(confidence, _, _)| *confidence)
            .collect();
        (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
    };
    println!(
        "{:<10} {:<5} {:>5} {:>6} {:>6} {:>6} {:>6} | {:>6} {:>6} | {:>6} {:>6} | {:>6}/{:<6}",
        corpus.to_string(),
        window,
        row.key.sounds,
        pct(row.key.coverage),
        pct(row.key.exact),
        pct(row.key.same_scale),
        pct(row.key.mirex),
        row.atonal.sounds,
        pct(row.atonal.false_key_rate),
        pct(metrics::expected_calibration_error(
            keyed_points.iter().copied()
        )),
        if false_keys.is_empty() {
            "-".to_string()
        } else {
            pct(metrics::expected_calibration_error(reweighted))
        },
        pct(mean_confidence(true)),
        pct(mean_confidence(false)),
    );
}

/// Keyed points plus atonal false keys (always wrong) carrying `ATONAL_WEIGHT_SHARE` of the weight.
fn reweighted_points(keyed: &[(f64, bool, f64)], false_keys: &[f64]) -> Vec<(f64, bool, f64)> {
    let atonal_weight = ATONAL_WEIGHT_SHARE / (1.0 - ATONAL_WEIGHT_SHARE) * keyed.len() as f64
        / false_keys.len().max(1) as f64;
    keyed
        .iter()
        .copied()
        .chain(
            false_keys
                .iter()
                .map(|&confidence| (confidence, false, atonal_weight)),
        )
        .collect()
}

fn count_decodable(items: &[KeyItem], args: &CorpusArgs) {
    let outcomes = run::par_map(items, |item| {
        key_corpus::prepare(item, Window::Full, args.min_seconds)
            .map(|prepared| prepared.source_seconds)
    });
    for corpus in &args.datasets {
        let mut decoded = 0;
        let mut seconds = 0.0;
        let mut failed = Vec::new();
        for (item, outcome) in items.iter().zip(&outcomes) {
            if item.corpus != *corpus {
                continue;
            }
            match outcome {
                Ok(length) => {
                    decoded += 1;
                    seconds += length;
                }
                Err(error) => failed.push(format!("{}: {error}", item.id)),
            }
        }
        println!(
            "{corpus}: decoded {decoded} ({:.0} s of audio, mean {:.1} s), failed {}",
            seconds,
            seconds / decoded.max(1) as f32,
            failed.len()
        );
        for failure in failed.iter().take(crate::MAX_LISTED_FAILURES) {
            println!("  {failure}");
        }
    }
}

/// Native cost of pushing a 30 s window and of one `estimate()` call on its history.
fn print_timing(item: &KeyItem) -> Result<(), String> {
    let prepared = key_corpus::prepare(item, Window::ThirtySeconds, 0.0)?;
    let seconds = prepared.samples.len() as f32 / ANALYSIS_SAMPLE_RATE as f32;
    let mut detector = KeyDetector::new(ANALYSIS_SAMPLE_RATE);
    let pushed = Instant::now();
    for chunk in prepared.samples.chunks(run::chunk_len()) {
        detector.push(chunk);
    }
    let push_seconds = pushed.elapsed().as_secs_f64();
    let estimated = Instant::now();
    for _ in 0..ESTIMATE_TIMING_CALLS {
        std::hint::black_box(detector.estimate());
    }
    let estimate_seconds = estimated.elapsed().as_secs_f64() / f64::from(ESTIMATE_TIMING_CALLS);
    println!(
        "\ntiming ({} {}, {seconds:.1} s history): push {:.3} ms per audio second, estimate() {:.1} µs",
        item.corpus,
        item.id,
        1e3 * push_seconds / f64::from(seconds),
        1e6 * estimate_seconds
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn false_keys_carry_the_atonal_share_of_the_weight() {
        let keyed = [
            (0.9, true, 1.0),
            (0.4, false, 1.0),
            (0.7, true, 1.0),
            (0.6, true, 1.0),
        ];
        let points = reweighted_points(&keyed, &[0.5, 0.3]);
        let total: f64 = points.iter().map(|(_, _, weight)| weight).sum();
        let atonal: f64 = points[keyed.len()..].iter().map(|(_, _, w)| w).sum();
        assert!((atonal / total - ATONAL_WEIGHT_SHARE).abs() < 1e-9);
        assert!(points[keyed.len()..].iter().all(|(_, correct, _)| !correct));
    }
}
