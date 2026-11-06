//! The detector configs the bench sweeps, each with a name that says what it is.

use super::detect::{Approach, FeatureSet, Gate, GridRule, NoveltyRule, RecentRule, TempoCheck};
use super::production;
use super::trace::RECENT_TEMPO_SECONDS;

/// Name prefixes of the approach families, in the order they're reported.
pub const FAMILIES: [&str; 5] = [
    "recent",
    "novelty",
    "gated-novelty",
    "grid",
    production::CONFIG,
];

/// The exploration's pick: split when the last 6 s of onsets confidently show a different beat.
pub const RECOMMENDED_CONFIG: &str = "grid tempo6s@0.6 nophase n4s";

/// Ticks of window before the recent stretch a comparison needs (8 s).
const MIN_OLD_TICKS: usize = 16;
/// A stricter variant: 20 s of window before the recent stretch.
const LONG_MIN_OLD_TICKS: usize = 40;
const MIN_TONAL_SECONDS: f32 = 1.0;
/// Index into `RECENT_TEMPO_SECONDS` of the recent tempo compared with the window's.
const RECENT_TEMPO_INDEX: usize = 1;

fn tempo_check(min_confidence: f32) -> TempoCheck {
    TempoCheck {
        recent_index: RECENT_TEMPO_INDEX,
        min_confidence,
    }
}

#[derive(Clone, Copy)]
enum KeySignal {
    None,
    /// The old key's scale trails the recent best by more than this, and the recent best scale
    /// holds for the whole sustain.
    Gap(f32),
    /// A gap where both sides' best key also clears a separation margin.
    ClearGap(f32, f32),
    /// Recent and old treble chroma correlate below this.
    Chroma(f32),
}

impl KeySignal {
    fn name(self) -> String {
        match self {
            KeySignal::None => "nokey".to_string(),
            KeySignal::Gap(value) => format!("stablegap{value}"),
            KeySignal::ClearGap(value, separation) => format!("stablegap{value}sep{separation}"),
            KeySignal::Chroma(value) => format!("chroma<{value}"),
        }
    }
}

fn tempo_name(tempo: Option<TempoCheck>) -> String {
    tempo.map_or("notempo".to_string(), |check| {
        format!(
            "tempo{:.0}s@{}",
            RECENT_TEMPO_SECONDS[check.recent_index], check.min_confidence
        )
    })
}

pub fn all_configs() -> Vec<(String, Approach)> {
    let mut configs = recent_configs();
    configs.extend(novelty_configs());
    configs.extend(grid_configs());
    configs.push((production::CONFIG.to_string(), Approach::Core));
    configs
}

struct RecentPoint {
    recent_ticks: usize,
    key: KeySignal,
    tempo: Option<TempoCheck>,
    sustain_ticks: usize,
    min_old_ticks: usize,
}

fn recent_configs() -> Vec<(String, Approach)> {
    recent_grid()
        .into_iter()
        .map(|point| {
            let (key_gap, min_separation) = match point.key {
                KeySignal::Gap(value) => (Some(value), 0.0),
                KeySignal::ClearGap(value, separation) => (Some(value), separation),
                _ => (None, 0.0),
            };
            let rule = RecentRule {
                recent_ticks: point.recent_ticks,
                key_gap,
                chroma_correlation: match point.key {
                    KeySignal::Chroma(value) => Some(value),
                    _ => None,
                },
                tempo: point.tempo,
                min_tonal_seconds: MIN_TONAL_SECONDS,
                min_separation,
                sustain_ticks: point.sustain_ticks,
                min_old_ticks: point.min_old_ticks,
            };
            let recent = match point.key {
                KeySignal::None => String::new(),
                _ => format!("r{}s ", point.recent_ticks / 2),
            };
            let name = format!(
                "recent {recent}{} {} n{}s old{}s",
                point.key.name(),
                tempo_name(point.tempo),
                point.sustain_ticks / 2,
                point.min_old_ticks / 2
            );
            (name, Approach::Recent(rule))
        })
        .collect()
}

/// Tempo-only rules over every recent-tempo length, then key rules with and without a tempo check.
fn recent_grid() -> Vec<RecentPoint> {
    let mut points = Vec::new();
    let tempo_checks = [(0, 0.6), (1, 0.5), (1, 0.6), (1, 0.8), (2, 0.6), (3, 0.6)];
    for (recent_index, min_confidence) in tempo_checks {
        for sustain_ticks in [2, 4, 8, 12] {
            for min_old_ticks in [MIN_OLD_TICKS, LONG_MIN_OLD_TICKS] {
                points.push(RecentPoint {
                    recent_ticks: 0,
                    key: KeySignal::None,
                    tempo: Some(TempoCheck {
                        recent_index,
                        min_confidence,
                    }),
                    sustain_ticks,
                    min_old_ticks,
                });
            }
        }
    }
    let key_signals = [
        KeySignal::Gap(0.2),
        KeySignal::Gap(0.3),
        KeySignal::Gap(0.4),
        KeySignal::ClearGap(0.2, 0.05),
        KeySignal::ClearGap(0.2, 0.1),
        KeySignal::ClearGap(0.3, 0.1),
        KeySignal::Chroma(0.3),
    ];
    for recent_ticks in [8, 16, 32, 48, 60] {
        for key in key_signals {
            for tempo in [None, Some(tempo_check(0.6))] {
                for sustain_ticks in [4, 8, 16, 24, 40] {
                    for min_old_ticks in [MIN_OLD_TICKS, LONG_MIN_OLD_TICKS] {
                        points.push(RecentPoint {
                            recent_ticks,
                            key,
                            tempo,
                            sustain_ticks,
                            min_old_ticks,
                        });
                    }
                }
            }
        }
    }
    points
}

fn novelty_configs() -> Vec<(String, Approach)> {
    let feature_sets = [
        FeatureSet::Chroma,
        FeatureSet::Timbre,
        FeatureSet::Rhythm,
        FeatureSet::All,
    ];
    let mut configs = Vec::new();
    for half_width in [8, 16] {
        for features in feature_sets {
            for threshold in [0.05, 0.1, 0.2, 0.3, 0.4] {
                let rule = NoveltyRule {
                    half_width,
                    features,
                    threshold,
                    gate: None,
                };
                let name = format!(
                    "novelty k{}s {} t{threshold}",
                    half_width / 2,
                    features.name()
                );
                configs.push((name, Approach::Novelty(rule)));
            }
            for threshold in [0.05, 0.1, 0.2] {
                for key_gap in [0.15, 0.2, 0.3] {
                    for tempo in [None, Some(0.7)] {
                        for min_after_ticks in [8, 12] {
                            let gate = Gate {
                                key_gap,
                                tempo: tempo.map(tempo_check),
                                min_tonal_seconds: MIN_TONAL_SECONDS,
                                min_after_ticks,
                                max_after_ticks: 40,
                            };
                            let rule = NoveltyRule {
                                half_width,
                                features,
                                threshold,
                                gate: Some(gate),
                            };
                            let name = format!(
                                "gated-novelty k{}s {} t{threshold} gap{key_gap} {} after{}s",
                                half_width / 2,
                                features.name(),
                                tempo_name(tempo.map(tempo_check)),
                                min_after_ticks / 2
                            );
                            configs.push((name, Approach::Novelty(rule)));
                        }
                    }
                }
            }
        }
    }
    configs
}

fn grid_configs() -> Vec<(String, Approach)> {
    let mut configs = Vec::new();
    for tempo in [None, Some(0.6), Some(0.8)] {
        for phase_jump in [None, Some(0.15), Some(0.25), Some(0.35)] {
            if tempo.is_none() && phase_jump.is_none() {
                continue;
            }
            let saliences: &[f32] = if phase_jump.is_some() {
                &[0.1, 0.2]
            } else {
                &[0.0]
            };
            for &min_salience in saliences {
                for sustain_ticks in [4, 8, 12] {
                    let rule = GridRule {
                        tempo: tempo.map(tempo_check),
                        phase_jump,
                        min_salience,
                        sustain_ticks,
                        min_old_ticks: MIN_OLD_TICKS,
                    };
                    let phase = phase_jump.map_or("nophase".to_string(), |jump| {
                        format!("phase>{jump}@sal{min_salience}")
                    });
                    let name = format!(
                        "grid {} {phase} n{}s",
                        tempo_name(tempo.map(tempo_check)),
                        sustain_ticks / 2
                    );
                    configs.push((name, Approach::Grid(rule)));
                }
            }
        }
    }
    configs
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn config_names_are_unique_and_include_the_recommended_one() {
        let names: Vec<String> = all_configs().into_iter().map(|(name, _)| name).collect();
        let unique: HashSet<&String> = names.iter().collect();
        assert_eq!(unique.len(), names.len());
        assert!(names.iter().any(|name| name == RECOMMENDED_CONFIG));
    }
}
