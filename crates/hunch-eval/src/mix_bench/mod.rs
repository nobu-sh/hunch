//! Experimental bench for detecting track changes without a pause: synthetic mixes of Beatport
//! tracks, scored for how fast and where each approach splits, how often it splits single
//! tracks, and how the readout after a change compares with not splitting.
//!
//! Each clip is analyzed once into a `Trace`; detectors (`detect`, swept by `grid`) replay it the
//! way a session would. Downstream readouts re-run real `Analyzer`s from each split's change point.

mod detect;
mod features;
mod grid;
mod keys;
mod pool;
mod render;
mod score;
mod trace;

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::time::Instant;

use clap::Args;
use hunch_core::tonal::KeyDetectorConfig;
use hunch_core::{Analyzer, Estimate};
use rayon::prelude::*;

use self::detect::{Approach, FeatureSet, Prepared, Split};
use self::keys::Scorer;
use self::pool::{Category, PoolTrack};
use self::render::{MixSpec, Transition};
use self::score::{Accuracy, HitStats, Summary, SummaryInput, Truth, format_optional, quantile};
use self::trace::{Trace, estimates_at};
use crate::resample::ANALYSIS_SAMPLE_RATE;
use crate::{DEFAULT_DATA_DIR, SplitChoice, run};

/// Readouts are scored this long after the change point.
const AFTER_CHANGE_SECONDS: [f32; 5] = [5.0, 10.0, 20.0, 30.0, 45.0];
/// Single tracks are scored at these times.
const SINGLE_TRACK_SECONDS: [f32; 4] = [30.0, 60.0, 90.0, 115.0];
const FORGET_SECONDS: [f32; 4] = [30.0, 45.0, 60.0, 90.0];
/// Forgetting paired with each pick's splits in the downstream tables.
const COMBINED_FORGET_SECONDS: f32 = 30.0;
/// False-split budgets the per-category table reports the best recall at.
const BUDGETS: [f32; 4] = [0.25, 0.5, 1.0, 2.0];

#[derive(Args)]
pub struct MixBenchArgs {
    /// Folder holding `beatport-key/`.
    #[arg(long, default_value = DEFAULT_DATA_DIR)]
    data: PathBuf,

    /// Beatport split the mixes and single tracks come from. Tune on val only.
    #[arg(long, value_enum, default_value = "val")]
    split: SplitChoice,

    /// Mixes per (pair category, transition) cell.
    #[arg(long, default_value_t = 30)]
    per_cell: usize,

    /// Seed for pair choice, transition lengths and A's play time.
    #[arg(long, default_value_t = 1)]
    seed: u64,

    /// Stop after this many single tracks (lowest ids first).
    #[arg(long)]
    negatives: Option<usize>,

    /// False splits per hour of single tracks each approach's pick may make.
    #[arg(long, default_value_t = 0.5)]
    budget: f32,

    /// Single tracks timed for the CPU table.
    #[arg(long, default_value_t = 12)]
    cpu_tracks: usize,

    /// Misses and false splits listed for the overall pick.
    #[arg(long, default_value_t = 10)]
    examples: usize,

    /// Only score these named configs, `;`-separated, skipping the sweep. Names are as the sweep
    /// table prints them, e.g. "grid tempo6s@0.6 nophase n4s".
    #[arg(long, value_delimiter = ';')]
    configs: Vec<String>,
}

/// Readouts at the scored times: index 0 is A's final readout, then `AFTER_CHANGE_SECONDS`.
type Readouts = Vec<Option<Estimate>>;

struct MixRun {
    spec: MixSpec,
    truth: Truth,
    trace: Trace,
    /// One per `baselines()` entry.
    baselines: Vec<Readouts>,
}

struct SingleRun {
    track: usize,
    seconds: f32,
    trace: Trace,
    baselines: Vec<Readouts>,
}

/// The default key config with this forgetting, so rows don't shift when the default does.
fn key_config(forget_seconds: Option<f32>) -> KeyDetectorConfig {
    KeyDetectorConfig {
        forget_seconds,
        ..KeyDetectorConfig::default()
    }
}

/// No splitting, with and without key-evidence forgetting.
fn baselines() -> Vec<(String, KeyDetectorConfig)> {
    let mut list = vec![("no split".to_string(), key_config(None))];
    for seconds in FORGET_SECONDS {
        list.push((format!("forget {seconds:.0}s"), key_config(Some(seconds))));
    }
    list
}

fn mix_times(truth: &Truth) -> Vec<f32> {
    let mut times = vec![truth.fade_start];
    times.extend(AFTER_CHANGE_SECONDS.map(|after| truth.change_seconds + after));
    times
}

pub fn run(args: &MixBenchArgs) -> Result<(), String> {
    let started = Instant::now();
    let tracks = pool::load(&args.data, args.split)?;
    if tracks.is_empty() {
        return Err(format!(
            "no keyed Beatport tracks in split {}",
            args.split.name()
        ));
    }
    let specs = render::plan_mixes(&tracks, args.per_cell, args.seed);
    print_config(args, &tracks, &specs);

    let mixes = run_mixes(&specs, &tracks);
    let singles = run_singles(args, &tracks);
    let scorer = Scorer::new();
    let mix_preps: Vec<Prepared> = mixes
        .iter()
        .map(|mix| Prepared::new(&mix.trace, &scorer))
        .collect();
    let single_preps: Vec<Prepared> = singles
        .iter()
        .map(|single| Prepared::new(&single.trace, &scorer))
        .collect();
    println!(
        "\nanalyzed {} mixes and {} single tracks ({:.1} h of audio) in {:.0}s",
        mixes.len(),
        singles.len(),
        audio_hours(&mixes, &singles),
        started.elapsed().as_secs_f32()
    );

    let configs = chosen_configs(&args.configs)?;
    let mix_splits = splits_for(&mix_preps, &configs);
    let single_splits = splits_for(&single_preps, &configs);
    let truths: Vec<Truth> = mixes.iter().map(|mix| mix.truth.clone()).collect();
    let single_seconds: Vec<f32> = singles.iter().map(|single| single.seconds).collect();
    let summaries: Vec<Summary> = (0..configs.len())
        .map(|config| {
            score::summarize(SummaryInput {
                truths: &truths,
                mix_splits: &mix_splits[config],
                negative_splits: &single_splits[config],
                negative_seconds: &single_seconds,
            })
        })
        .collect();

    let picks = if args.configs.is_empty() {
        print_sweep(&configs, &summaries);
        print_budget_table(&configs, &summaries);
        pick_configs(&configs, &summaries, args.budget)
    } else {
        (0..configs.len()).collect()
    };
    for &pick in &picks {
        print_detail(&configs[pick].0, &summaries[pick]);
    }
    print_downstream(&DownstreamInput {
        tracks: &tracks,
        mixes: &mixes,
        singles: &singles,
        configs: &configs,
        picks: &picks,
        mix_splits: &mix_splits,
        single_splits: &single_splits,
    })?;
    if let Some(&best) = picks.first() {
        print_examples(&ExampleInput {
            name: &configs[best].0,
            mixes: &mixes,
            singles: &singles,
            tracks: &tracks,
            mix_splits: &mix_splits[best],
            single_splits: &single_splits[best],
            limit: args.examples,
        });
    }
    print_cpu(&CpuInput {
        tracks: &tracks,
        singles: &singles,
        count: args.cpu_tracks,
        configs: &configs,
        picks: &picks,
    })?;
    println!("\nwall clock {:.0}s", started.elapsed().as_secs_f32());
    Ok(())
}

/// Every swept config, or only the `names` asked for, in sweep order.
fn chosen_configs(names: &[String]) -> Result<Vec<(String, Approach)>, String> {
    let all = grid::all_configs();
    if names.is_empty() {
        return Ok(all);
    }
    let unknown: Vec<&String> = names
        .iter()
        .filter(|name| !all.iter().any(|(known, _)| known == *name))
        .collect();
    if !unknown.is_empty() {
        return Err(format!(
            "unknown configs {unknown:?}; names are as the sweep table prints them, e.g. \"{}\"",
            grid::RECOMMENDED_CONFIG
        ));
    }
    Ok(all
        .into_iter()
        .filter(|(name, _)| names.contains(name))
        .collect())
}

fn audio_hours(mixes: &[MixRun], singles: &[SingleRun]) -> f32 {
    let mix_seconds: f32 = mixes
        .iter()
        .map(|mix| mix.truth.change_seconds + render::B_SECONDS)
        .sum();
    let single_seconds: f32 = singles.iter().map(|single| single.seconds).sum();
    (mix_seconds + single_seconds) / 3600.0
}

fn print_config(args: &MixBenchArgs, tracks: &[PoolTrack], specs: &[MixSpec]) {
    let with_bpm = tracks.iter().filter(|track| track.bpm.is_some()).count();
    println!("# mix-bench");
    println!(
        "config: split {}, seed {}, {} per cell, budget {:.2} false splits/h, data {}",
        args.split.name(),
        args.seed,
        args.per_cell,
        args.budget,
        args.data.display()
    );
    println!(
        "pool: {} keyed Beatport tracks, {with_bpm} with a Beatport BPM (mixes use only those)",
        tracks.len()
    );
    println!(
        "mixes: A plays 45-75 s, then the transition, then B for {:.0} s after the change point",
        render::B_SECONDS
    );
    let mut counts: BTreeMap<(String, Category), usize> = BTreeMap::new();
    for spec in specs {
        *counts
            .entry((spec.transition.kind(), spec.category))
            .or_default() += 1;
    }
    println!(
        "\n| transition | {} |",
        Category::ALL.map(Category::name).join(" | ")
    );
    println!("|---|{}", "---|".repeat(Category::ALL.len()));
    let mut kinds: Vec<String> = Transition::PLAIN.iter().map(|t| t.kind()).collect();
    kinds.push(Transition::Dj(8.0).kind());
    for kind in kinds {
        let cells: Vec<String> = Category::ALL
            .iter()
            .map(|&category| {
                counts
                    .get(&(kind.clone(), category))
                    .copied()
                    .unwrap_or(0)
                    .to_string()
            })
            .collect();
        println!("| {kind} | {} |", cells.join(" | "));
    }
}

fn run_mixes(specs: &[MixSpec], tracks: &[PoolTrack]) -> Vec<MixRun> {
    let baselines = baselines();
    let outcomes = run::par_map(specs, |spec| {
        let mix = render::render(spec, tracks)?;
        let truth = Truth {
            category: spec.category,
            kind: spec.transition.kind(),
            change_seconds: mix.change_seconds,
            fade_start: mix.fade_start,
        };
        let times = mix_times(&truth);
        let readouts = baselines
            .iter()
            .map(|(_, config)| estimates_at(&mix.samples, *config, 0.0, &times))
            .collect();
        Ok(MixRun {
            spec: spec.clone(),
            truth,
            trace: trace::analyze(&mix.samples),
            baselines: readouts,
        })
    });
    keep_successes(outcomes, "mixes")
}

fn run_singles(args: &MixBenchArgs, tracks: &[PoolTrack]) -> Vec<SingleRun> {
    let baselines = baselines();
    let indices: Vec<usize> = (0..tracks.len())
        .take(args.negatives.unwrap_or(usize::MAX))
        .collect();
    let outcomes = run::par_map(&indices, |&track| {
        let samples = render::load_track(&tracks[track])?;
        let readouts = baselines
            .iter()
            .map(|(_, config)| estimates_at(&samples, *config, 0.0, &SINGLE_TRACK_SECONDS))
            .collect();
        Ok(SingleRun {
            track,
            seconds: samples.len() as f32 / ANALYSIS_SAMPLE_RATE as f32,
            trace: trace::analyze(&samples),
            baselines: readouts,
        })
    });
    keep_successes(outcomes, "single tracks")
}

fn keep_successes<T>(outcomes: Vec<Result<T, String>>, what: &str) -> Vec<T> {
    let mut kept = Vec::new();
    let mut failures = 0;
    for outcome in outcomes {
        match outcome {
            Ok(value) => kept.push(value),
            Err(error) => {
                failures += 1;
                if failures <= crate::MAX_LISTED_FAILURES {
                    eprintln!("warning: skipping one of the {what}: {error}");
                }
            }
        }
    }
    if failures > 0 {
        eprintln!("{failures} {what} skipped");
    }
    kept
}

/// Splits per config, per trace.
fn splits_for(preps: &[Prepared], configs: &[(String, Approach)]) -> Vec<Vec<Vec<Split>>> {
    let per_trace: Vec<Vec<Vec<Split>>> = preps
        .par_iter()
        .map(|prep| {
            let mut curves: HashMap<(usize, FeatureSet), Vec<f32>> = HashMap::new();
            configs
                .iter()
                .map(|(_, approach)| {
                    let curve: &[f32] = match approach {
                        Approach::Novelty(rule) => curves
                            .entry((rule.half_width, rule.features))
                            .or_insert_with(|| {
                                detect::novelty_curve(prep, rule.half_width, rule.features)
                            }),
                        _ => &[],
                    };
                    detect::run_session(prep, approach, curve)
                })
                .collect()
        })
        .collect();
    (0..configs.len())
        .map(|config| {
            per_trace
                .iter()
                .map(|splits| splits[config].clone())
                .collect()
        })
        .collect()
}

fn percent(value: f32) -> String {
    format!("{:.0}%", 100.0 * value)
}

fn print_sweep(configs: &[(String, Approach)], summaries: &[Summary]) {
    println!("\n## Sweep: recall on changing mixes vs false splits per hour of single tracks");
    println!(
        "Pareto front per approach (no other config of it has both more recall and fewer false splits)."
    );
    for family in grid::FAMILIES {
        println!("\n### {family}");
        println!(
            "| config | recall | median delay s | median cp error s | false/h single | splits in A /h | same-pair splits |"
        );
        println!("|---|---|---|---|---|---|---|");
        let mut members: Vec<usize> = (0..configs.len())
            .filter(|&i| configs[i].0.starts_with(family))
            .collect();
        members.sort_by(|&a, &b| {
            summaries[a]
                .false_per_hour()
                .total_cmp(&summaries[b].false_per_hour())
                .then(
                    summaries[b]
                        .changes
                        .recall()
                        .total_cmp(&summaries[a].changes.recall()),
                )
        });
        let mut best_recall = -1.0;
        for index in members {
            let summary = &summaries[index];
            if summary.changes.recall() <= best_recall {
                continue;
            }
            best_recall = summary.changes.recall();
            println!(
                "| {} | {} |",
                configs[index].0,
                summary_cells(summary).join(" | ")
            );
        }
    }
}

/// Per approach family and budget: the best recall on each changing category, from any config
/// within the budget (each cell may come from a different config).
fn print_budget_table(configs: &[(String, Approach)], summaries: &[Summary]) {
    println!("\n## Best recall per category within a false-split budget");
    println!("Each cell is the best config of that family for that column alone.");
    let changing: Vec<Category> = Category::ALL
        .into_iter()
        .filter(|category| category.is_change())
        .collect();
    let headers: Vec<&str> = changing.iter().map(|category| category.name()).collect();
    println!(
        "| family | max false/h | all changes | {} |",
        headers.join(" | ")
    );
    println!("|---|---|---|{}", "---|".repeat(changing.len()));
    for family in grid::FAMILIES {
        for budget in BUDGETS {
            let members: Vec<&Summary> = (0..configs.len())
                .filter(|&i| configs[i].0.starts_with(family))
                .map(|i| &summaries[i])
                .filter(|summary| summary.false_per_hour() <= budget)
                .collect();
            let best = |recall: &dyn Fn(&Summary) -> f32| {
                members
                    .iter()
                    .map(|summary| recall(summary))
                    .fold(0.0, f32::max)
            };
            let cells: Vec<String> = changing
                .iter()
                .map(|category| {
                    percent(best(&|summary: &Summary| {
                        summary
                            .by_category
                            .get(category)
                            .map_or(0.0, HitStats::recall)
                    }))
                })
                .collect();
            println!(
                "| {family} | {budget} | {} | {} |",
                percent(best(&|summary: &Summary| summary.changes.recall())),
                cells.join(" | ")
            );
        }
    }
}

fn summary_cells(summary: &Summary) -> Vec<String> {
    let changes = &summary.changes;
    vec![
        percent(changes.recall()),
        format_optional(quantile(&changes.delays, 0.5)),
        format_optional(quantile(&changes.errors, 0.5)),
        format!("{:.2}", summary.false_per_hour()),
        format!("{:.2}", summary.early_per_hour()),
        format!(
            "{:.2}/mix",
            summary.same_pairs.hits as f32 / summary.same_pairs.mixes.max(1) as f32
        ),
    ]
}

/// The highest-recall config of each approach family within the false-split budget, best first.
fn pick_configs(configs: &[(String, Approach)], summaries: &[Summary], budget: f32) -> Vec<usize> {
    let mut picks: Vec<usize> = grid::FAMILIES
        .iter()
        .filter_map(|family| {
            (0..configs.len())
                .filter(|&i| configs[i].0.starts_with(family))
                .filter(|&i| summaries[i].false_per_hour() <= budget)
                .max_by(|&a, &b| {
                    summaries[a]
                        .changes
                        .recall()
                        .total_cmp(&summaries[b].changes.recall())
                })
        })
        .collect();
    picks.sort_by(|&a, &b| {
        summaries[b]
            .changes
            .recall()
            .total_cmp(&summaries[a].changes.recall())
    });
    println!("\n## Picks within {budget:.2} false splits/h");
    for &pick in &picks {
        println!("- {}", configs[pick].0);
    }
    picks
}

fn hit_cells(stats: &HitStats) -> Vec<String> {
    vec![
        stats.mixes.to_string(),
        percent(stats.recall()),
        format_optional(quantile(&stats.delays, 0.5)),
        format_optional(quantile(&stats.delays, 0.9)),
        format_optional(quantile(&stats.errors, 0.5)),
        format!(
            "{} .. {}",
            format_optional(quantile(&stats.errors, 0.1)),
            format_optional(quantile(&stats.errors, 0.9))
        ),
        format!("{:.2}", stats.extra as f32 / stats.mixes.max(1) as f32),
    ]
}

fn print_detail(name: &str, summary: &Summary) {
    println!("\n### {name}");
    println!(
        "false splits: {:.2}/h on {:.1} h of single tracks ({} splits), {:.2}/h in A before the transition",
        summary.false_per_hour(),
        summary.negative_hours,
        summary.negative_splits,
        summary.early_per_hour()
    );
    println!(
        "| group | mixes | recall | delay p50 s | delay p90 s | cp error p50 s | cp error p10..p90 s | extra splits/mix |"
    );
    println!("|---|---|---|---|---|---|---|---|");
    println!(
        "| all changes | {} |",
        hit_cells(&summary.changes).join(" | ")
    );
    for (category, stats) in &summary.by_category {
        println!("| {} | {} |", category.name(), hit_cells(stats).join(" | "));
    }
    for (kind, stats) in &summary.by_kind {
        println!("| {kind} | {} |", hit_cells(stats).join(" | "));
    }
    println!(
        "| same key+tempo (bonus) | {} |",
        hit_cells(&summary.same_pairs).join(" | ")
    );
}

struct DownstreamInput<'a> {
    tracks: &'a [PoolTrack],
    mixes: &'a [MixRun],
    singles: &'a [SingleRun],
    configs: &'a [(String, Approach)],
    picks: &'a [usize],
    mix_splits: &'a [Vec<Vec<Split>>],
    single_splits: &'a [Vec<Vec<Split>>],
}

/// Readouts at `times` for a session that splits at `splits`: each time reads the window open
/// then, which starts at the change point of the last split detected by that time.
fn split_readouts(
    samples: &[f32],
    splits: &[Split],
    times: &[f32],
    key_config: KeyDetectorConfig,
) -> Readouts {
    let start_at = |time: f32| {
        splits
            .iter()
            .filter(|split| Trace::seconds_of(split.detect_tick) <= time)
            .map(|split| Trace::seconds_of(split.change_tick))
            .next_back()
            .unwrap_or(0.0)
    };
    let mut readouts = vec![None; times.len()];
    let mut starts: Vec<f32> = times.iter().map(|&time| start_at(time)).collect();
    starts.dedup();
    for start in starts {
        let indices: Vec<usize> = (0..times.len())
            .filter(|&i| start_at(times[i]) == start)
            .collect();
        let at: Vec<f32> = indices.iter().map(|&i| times[i]).collect();
        let estimates = estimates_at(samples, key_config, start, &at);
        for (index, estimate) in indices.into_iter().zip(estimates) {
            readouts[index] = estimate;
        }
    }
    readouts
}

/// A's final readout with splits: the window that the hit split closed, read at its change
/// point. Without a hit, the readout when B started.
fn split_a_final(
    samples: &[f32],
    truth: &Truth,
    splits: &[Split],
    key_config: KeyDetectorConfig,
) -> Option<Estimate> {
    let hit = splits
        .iter()
        .position(|split| Trace::seconds_of(split.detect_tick) >= truth.fade_start);
    let Some(hit) = hit else {
        return split_readouts(samples, splits, &[truth.fade_start], key_config).remove(0);
    };
    let window_start = hit.checked_sub(1).map_or(0.0, |previous| {
        Trace::seconds_of(splits[previous].change_tick)
    });
    let change = Trace::seconds_of(splits[hit].change_tick);
    estimates_at(samples, key_config, window_start, &[change]).remove(0)
}

#[derive(Default, Clone)]
struct MixAccuracy {
    a_final: Accuracy,
    after: [Accuracy; AFTER_CHANGE_SECONDS.len()],
}

impl MixAccuracy {
    fn add(&mut self, mix: &MixRun, readouts: &Readouts) {
        self.a_final
            .add(readouts[0].as_ref(), &mix.spec.a_keys, mix.spec.a_bpm);
        for (accuracy, estimate) in self.after.iter_mut().zip(&readouts[1..]) {
            accuracy.add(estimate.as_ref(), &mix.spec.b_keys, mix.spec.b_bpm);
        }
    }

    fn merge(&mut self, other: &Self) {
        self.a_final.merge(&other.a_final);
        for (mine, theirs) in self.after.iter_mut().zip(&other.after) {
            mine.merge(theirs);
        }
    }
}

/// A split config scored with a given analyzer config.
struct SplitVariant {
    name: String,
    pick: usize,
    key_config: KeyDetectorConfig,
}

/// Every pick keeping all key evidence, then with key forgetting on top.
fn split_variants(input: &DownstreamInput) -> Vec<SplitVariant> {
    let forgetting = key_config(Some(COMBINED_FORGET_SECONDS));
    let plain = input.picks.iter().map(|&pick| SplitVariant {
        name: input.configs[pick].0.clone(),
        pick,
        key_config: key_config(None),
    });
    let combined = input.picks.iter().map(|&pick| SplitVariant {
        name: format!(
            "{} + forget {COMBINED_FORGET_SECONDS:.0}s",
            input.configs[pick].0
        ),
        pick,
        key_config: forgetting,
    });
    plain.chain(combined).collect()
}

fn print_downstream(input: &DownstreamInput) -> Result<(), String> {
    let baseline_names: Vec<String> = baselines().into_iter().map(|(name, _)| name).collect();
    let variants = split_variants(input);
    let mut rows: Vec<String> = baseline_names.clone();
    rows.extend(variants.iter().map(|variant| variant.name.clone()));

    // Per row: accuracy over changing mixes and over same key+tempo mixes.
    let mut changing = vec![MixAccuracy::default(); rows.len()];
    let mut same = vec![MixAccuracy::default(); rows.len()];
    for mix in input.mixes {
        let target = if mix.truth.category.is_change() {
            &mut changing
        } else {
            &mut same
        };
        for (row, readouts) in mix.baselines.iter().enumerate() {
            target[row].add(mix, readouts);
        }
    }
    let indices: Vec<usize> = (0..input.mixes.len()).collect();
    let split_rows: Vec<Vec<MixAccuracy>> = run::par_map(&indices, |&mix_index| {
        let mix = &input.mixes[mix_index];
        let rendered = render::render(&mix.spec, input.tracks)?;
        let times = mix_times(&mix.truth);
        Ok(variants
            .iter()
            .map(|variant| {
                let splits = &input.mix_splits[variant.pick][mix_index];
                let config = variant.key_config;
                let mut readouts = split_readouts(&rendered.samples, splits, &times, config);
                readouts[0] = split_a_final(&rendered.samples, &mix.truth, splits, config);
                let mut accuracy = MixAccuracy::default();
                accuracy.add(mix, &readouts);
                accuracy
            })
            .collect())
    })
    .into_iter()
    .collect::<Result<_, String>>()?;
    for (mix, per_variant) in input.mixes.iter().zip(&split_rows) {
        let target = if mix.truth.category.is_change() {
            &mut changing
        } else {
            &mut same
        };
        for (offset, accuracy) in per_variant.iter().enumerate() {
            target[baseline_names.len() + offset].merge(accuracy);
        }
    }

    for (title, table) in [
        ("changing mixes", &changing),
        ("same key+tempo mixes", &same),
    ] {
        println!("\n## Downstream readout, {title}");
        println!(
            "Key: same scale as the label. BPM: within 4% of Beatport's BPM. A final: A's last readout (with splits: the closed window at its change point)."
        );
        let after_headers: Vec<String> = AFTER_CHANGE_SECONDS
            .iter()
            .map(|seconds| format!("key B+{seconds:.0}s"))
            .collect();
        println!(
            "| readout | key A final | {} | bpm A final | bpm B+10s | bpm B+30s |",
            after_headers.join(" | ")
        );
        println!(
            "|---|---|{}---|---|---|",
            "---|".repeat(AFTER_CHANGE_SECONDS.len())
        );
        for (name, accuracy) in rows.iter().zip(table.iter()) {
            let keys: Vec<String> = accuracy
                .after
                .iter()
                .map(|after| percent(after.key()))
                .collect();
            println!(
                "| {name} | {} | {} | {} | {} | {} |",
                percent(accuracy.a_final.key()),
                keys.join(" | "),
                percent(accuracy.a_final.bpm()),
                percent(accuracy.after[1].bpm()),
                percent(accuracy.after[3].bpm())
            );
        }
    }

    print_single_downstream(input, &rows, &variants)
}

fn print_single_downstream(
    input: &DownstreamInput,
    rows: &[String],
    variants: &[SplitVariant],
) -> Result<(), String> {
    let baseline_count = rows.len() - variants.len();
    let mut table = vec![[Accuracy::default(); SINGLE_TRACK_SECONDS.len()]; rows.len()];
    for single in input.singles {
        let track = &input.tracks[single.track];
        for (row, readouts) in single.baselines.iter().enumerate() {
            for (accuracy, estimate) in table[row].iter_mut().zip(readouts) {
                accuracy.add(estimate.as_ref(), &track.keys, track.bpm);
            }
        }
    }
    let indices: Vec<usize> = (0..input.singles.len()).collect();
    let per_single = run::par_map(&indices, |&position| {
        let single = &input.singles[position];
        let samples = render::load_track(&input.tracks[single.track])?;
        Ok(variants
            .iter()
            .map(|variant| {
                split_readouts(
                    &samples,
                    &input.single_splits[variant.pick][position],
                    &SINGLE_TRACK_SECONDS,
                    variant.key_config,
                )
            })
            .collect::<Vec<Readouts>>())
    })
    .into_iter()
    .collect::<Result<Vec<_>, String>>()?;
    for (single, per_variant) in input.singles.iter().zip(&per_single) {
        let track = &input.tracks[single.track];
        for (offset, readouts) in per_variant.iter().enumerate() {
            for (accuracy, estimate) in table[baseline_count + offset].iter_mut().zip(readouts) {
                accuracy.add(estimate.as_ref(), &track.keys, track.bpm);
            }
        }
    }
    println!("\n## Single tracks: readout accuracy (cost of forgetting and false splits)");
    let headers: Vec<String> = SINGLE_TRACK_SECONDS
        .iter()
        .map(|seconds| format!("key {seconds:.0}s"))
        .collect();
    let bpm_headers: Vec<String> = SINGLE_TRACK_SECONDS
        .iter()
        .map(|seconds| format!("bpm {seconds:.0}s"))
        .collect();
    println!(
        "| readout | {} | {} |",
        headers.join(" | "),
        bpm_headers.join(" | ")
    );
    println!("|---|{}", "---|".repeat(2 * SINGLE_TRACK_SECONDS.len()));
    for (name, accuracies) in rows.iter().zip(&table) {
        let keys: Vec<String> = accuracies.iter().map(|a| percent(a.key())).collect();
        let bpms: Vec<String> = accuracies.iter().map(|a| percent(a.bpm())).collect();
        println!("| {name} | {} | {} |", keys.join(" | "), bpms.join(" | "));
    }
    Ok(())
}

struct ExampleInput<'a> {
    name: &'a str,
    mixes: &'a [MixRun],
    singles: &'a [SingleRun],
    tracks: &'a [PoolTrack],
    mix_splits: &'a [Vec<Split>],
    single_splits: &'a [Vec<Split>],
    limit: usize,
}

fn print_examples(input: &ExampleInput) {
    println!("\n## Examples for {}", input.name);
    println!("\nMissed changes (mix: A id -> B id, transition, category, change at):");
    let misses = input
        .mixes
        .iter()
        .zip(input.mix_splits)
        .filter(|(mix, splits)| {
            mix.truth.category.is_change() && score::classify(&mix.truth, splits).hit.is_none()
        });
    for (mix, splits) in misses.take(input.limit) {
        let found: Vec<String> = splits
            .iter()
            .map(|split| format!("{:.1}s", Trace::seconds_of(split.detect_tick)))
            .collect();
        println!(
            "- mix {}: {} -> {}, {}, {}, change {:.1}s, A {:?} {:?} BPM, B {:?} {:?} BPM, splits at [{}]",
            mix.spec.id,
            mix.spec.a_id,
            mix.spec.b_id,
            mix.truth.kind,
            mix.truth.category.name(),
            mix.truth.change_seconds,
            mix.spec
                .a_keys
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            mix.spec.a_bpm,
            mix.spec
                .b_keys
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            mix.spec.b_bpm,
            found.join(", ")
        );
    }
    println!(
        "\nSplits inside A before the transition (mix: A id, detected at, change point, cause):"
    );
    let early = input
        .mixes
        .iter()
        .zip(input.mix_splits)
        .flat_map(|(mix, splits)| splits.iter().map(move |split| (mix, split)))
        .filter(|(mix, split)| Trace::seconds_of(split.detect_tick) < mix.truth.fade_start);
    for (mix, split) in early.take(input.limit) {
        println!(
            "- mix {}: A {}, detected {:.1}s, change point {:.1}s, {:?} (transition at {:.1}s)",
            mix.spec.id,
            mix.spec.a_id,
            Trace::seconds_of(split.detect_tick),
            Trace::seconds_of(split.change_tick),
            split.cause,
            mix.truth.fade_start
        );
    }
    println!("\nFalse splits on single tracks (track id, detected at, change point, cause):");
    let false_splits = input
        .singles
        .iter()
        .zip(input.single_splits)
        .flat_map(|(single, splits)| splits.iter().map(move |split| (single, split)));
    for (single, split) in false_splits.take(input.limit) {
        let track = &input.tracks[single.track];
        println!(
            "- {} ({}): detected {:.1}s, change point {:.1}s, {:?}",
            track.id,
            track
                .audio_path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("?"),
            Trace::seconds_of(split.detect_tick),
            Trace::seconds_of(split.change_tick),
            split.cause
        );
    }
}

struct CpuInput<'a> {
    tracks: &'a [PoolTrack],
    singles: &'a [SingleRun],
    count: usize,
    configs: &'a [(String, Approach)],
    picks: &'a [usize],
}

/// Native single-thread milliseconds per second of audio for the analyzer and each addition.
fn print_cpu(input: &CpuInput) -> Result<(), String> {
    let chosen: Vec<&SingleRun> = input.singles.iter().take(input.count).collect();
    let audio: Vec<Vec<f32>> = chosen
        .iter()
        .map(|single| render::load_track(&input.tracks[single.track]))
        .collect::<Result<_, _>>()?;
    let seconds: f32 = audio
        .iter()
        .map(|samples| samples.len() as f32 / ANALYSIS_SAMPLE_RATE as f32)
        .sum();
    let time = |job: &dyn Fn(&[f32])| {
        let started = Instant::now();
        for samples in &audio {
            job(samples);
        }
        1000.0 * started.elapsed().as_secs_f32() / seconds
    };

    let plain = time(&|samples| {
        let mut analyzer = Analyzer::new(ANALYSIS_SAMPLE_RATE);
        samples.chunks(run::chunk_len()).for_each(|chunk| {
            analyzer.push(chunk);
        });
    });
    let keeping = time(&|samples| {
        let mut analyzer = Analyzer::with_key_config(ANALYSIS_SAMPLE_RATE, key_config(None));
        samples.chunks(run::chunk_len()).for_each(|chunk| {
            analyzer.push(chunk);
        });
    });
    let with_recent = |recent_seconds: Option<f32>| {
        time(&|samples| {
            let config = KeyDetectorConfig {
                track_blocks: true,
                ..KeyDetectorConfig::default()
            };
            let mut analyzer = Analyzer::with_key_config(ANALYSIS_SAMPLE_RATE, config);
            for chunk in samples.chunks(run::chunk_len()) {
                if analyzer.push(chunk).is_none() {
                    continue;
                }
                analyzer.key_detector_mut().take_block();
                if let Some(seconds) = recent_seconds {
                    std::hint::black_box(analyzer.tempo_detector().estimate_recent(seconds));
                }
            }
        })
    };
    let blocks = with_recent(None);
    let recent_short = with_recent(Some(trace::RECENT_TEMPO_SECONDS[1]));
    let recent_long = with_recent(Some(trace::RECENT_TEMPO_SECONDS[3]));
    let front_end = time(&|samples| {
        std::hint::black_box(features::flux_of(samples));
    });
    let scorer = Scorer::new();
    let detectors: Vec<(String, f32)> = input
        .picks
        .iter()
        .map(|&pick| {
            let (name, approach) = &input.configs[pick];
            let started = Instant::now();
            for single in &chosen {
                let prep = Prepared::new(&single.trace, &scorer);
                let curve = match approach {
                    Approach::Novelty(rule) => {
                        detect::novelty_curve(&prep, rule.half_width, rule.features)
                    }
                    _ => Vec::new(),
                };
                std::hint::black_box(detect::run_session(&prep, approach, &curve));
            }
            (
                name.clone(),
                1000.0 * started.elapsed().as_secs_f32() / seconds,
            )
        })
        .collect();

    println!(
        "\n## CPU, native release build, one thread, ms per audio second ({seconds:.0} s of audio)"
    );
    println!("| part | ms/s | vs analyzer |");
    println!("|---|---|---|");
    println!("| analyzer (shipped) | {plain:.3} | 100% |");
    let rows = [
        ("analyzer keeping all key evidence".to_string(), keeping),
        ("analyzer + 0.5 s chroma blocks".to_string(), blocks),
        (
            format!(
                "analyzer + blocks + {:.0} s recent tempo per estimate",
                trace::RECENT_TEMPO_SECONDS[1]
            ),
            recent_short,
        ),
        (
            format!(
                "analyzer + blocks + {:.0} s recent tempo per estimate",
                trace::RECENT_TEMPO_SECONDS[3]
            ),
            recent_long,
        ),
        (
            "bench onset/timbre front end (novelty, beat grid)".to_string(),
            front_end,
        ),
    ];
    for (name, value) in rows {
        println!("| {name} | {value:.3} | {:.0}% |", 100.0 * value / plain);
    }
    for (name, value) in detectors {
        println!(
            "| detector `{name}` over a trace (prep, novelty, decisions) | {value:.3} | {:.0}% |",
            100.0 * value / plain
        );
    }
    Ok(())
}
