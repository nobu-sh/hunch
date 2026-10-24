//! Freesound Loop Dataset (FSL10K) loader: metadata, expert annotations, labels and splits.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use clap::ValueEnum;
use hunch_core::{Key, Mode, PitchClass};
use serde::Deserialize;

/// Two tempi count as the same within this relative tolerance (the MIREX Accuracy1 window).
pub const TEMPO_TOLERANCE: f32 = 0.04;
const HIP_HOP_GENRE: &str = "hip hop";
/// Target share of all sounds per split, in percent.
const SPLIT_PERCENTS: [(Split, u64); 3] = [(Split::Train, 70), (Split::Val, 15), (Split::Test, 15)];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, ValueEnum)]
pub enum Split {
    Train,
    Val,
    Test,
}

impl Split {
    pub const ALL: [Split; 3] = [Split::Train, Split::Val, Split::Test];
}

/// Assigns every creator to one split, so one creator's loops never straddle splits.
pub fn assign_splits<'a>(usernames: impl IntoIterator<Item = &'a str>) -> HashMap<&'a str, Split> {
    assign_groups(usernames, &SPLIT_PERCENTS)
}

/// Assigns every group (one entry per member) to one split with roughly the `targets` share of
/// members, in percent.
///
/// Hashing groups straight into buckets lets one large group (one FSL10K creator has ~1,650 loops)
/// skew a split badly. Instead groups are placed largest first, each into the split furthest below
/// its target share. Equal sizes are ordered by FNV-1a of the name, so the result is deterministic.
pub fn assign_groups<'a>(
    members: impl IntoIterator<Item = &'a str>,
    targets: &[(Split, u64)],
) -> HashMap<&'a str, Split> {
    let mut sizes: HashMap<&str, u64> = HashMap::new();
    for name in members {
        *sizes.entry(name).or_default() += 1;
    }
    let total: u64 = sizes.values().sum();
    let mut groups: Vec<(&str, u64)> = sizes.into_iter().collect();
    groups.sort_by_key(|&(name, size)| (Reverse(size), fnv1a(name.as_bytes())));

    let mut filled = vec![0u64; targets.len()];
    let mut splits = HashMap::with_capacity(groups.len());
    for (name, size) in groups {
        // Shortfall from the target, scaled by 100 to stay in integers.
        let shortfall =
            |index: usize| (targets[index].1 * total) as i64 - (filled[index] * 100) as i64;
        let Some(index) =
            (0..targets.len()).max_by_key(|&index| (shortfall(index), Reverse(index)))
        else {
            break;
        };
        filled[index] += size;
        splits.insert(name, targets[index].0);
    }
    splits
}

impl fmt::Display for Split {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Split::Train => "train",
            Split::Val => "val",
            Split::Test => "test",
        })
    }
}

/// 64-bit FNV-1a. Inline rather than `DefaultHasher`, whose output may change between Rust releases.
fn fnv1a(bytes: &[u8]) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    bytes.iter().fold(OFFSET_BASIS, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(PRIME)
    })
}

/// Expert tempo label of a sound.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExpertTempo {
    /// No usable expert annotation.
    Unlabeled,
    /// Annotators gave no tempo, or disagreed beyond `TEMPO_TOLERANCE`.
    Ambiguous,
    Bpm(f32),
}

/// Expert key label of a sound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyLabel {
    /// No usable expert annotation.
    Unlabeled,
    /// Every annotator said the loop has no key (drums, fx, noise).
    Atonal,
    /// Disagreement, `unknown`, or a mix of keyed and keyless answers. Excluded from key metrics.
    Ambiguous,
    Key(Key),
}

impl fmt::Display for KeyLabel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KeyLabel::Unlabeled => Ok(()),
            KeyLabel::Atonal => f.write_str("atonal"),
            KeyLabel::Ambiguous => f.write_str("ambiguous"),
            KeyLabel::Key(key) => write!(f, "{key}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Labels {
    /// The BPM the creator labeled the loop with: the convention the detector should match.
    pub tempo_creator: Option<f32>,
    pub tempo_expert: ExpertTempo,
    pub key: KeyLabel,
}

#[derive(Debug, Clone)]
pub struct Sound {
    pub id: u64,
    pub username: String,
    pub split: Split,
    pub audio_path: PathBuf,
    pub labels: Labels,
    pub genres: BTreeSet<String>,
    pub is_hip_hop: bool,
}

/// Every FSL10K sound that has audio, sorted by id.
pub struct Dataset {
    pub sounds: Vec<Sound>,
    /// Metadata ids with no matching audio file.
    pub missing_audio: Vec<u64>,
}

#[derive(Deserialize)]
struct MetadataEntry {
    username: String,
    #[serde(default)]
    annotations: MetadataAnnotations,
}

#[derive(Deserialize, Default)]
struct MetadataAnnotations {
    #[serde(default)]
    bpm: f32,
}

/// One annotator's judgment of one sound, as stored in `sound-<id>.json`.
#[derive(Debug, Clone, Deserialize)]
pub struct ExpertAnnotation {
    #[serde(default)]
    pub bpm: String,
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub genres: Vec<String>,
    #[serde(default)]
    pub discard: bool,
}

impl Dataset {
    /// Loads FSL10K from `data_dir`, which holds `FSL10K/` and `fsl10k-annotations/`.
    pub fn load(data_dir: &Path) -> Result<Self, String> {
        let root = data_dir.join("FSL10K");
        let metadata: HashMap<String, MetadataEntry> = read_json(&root.join("metadata.json"))?;
        let parent_genres: HashMap<String, Vec<String>> =
            read_json(&root.join("parent_genres.json"))?;
        let audio = index_audio(&root.join("audio").join("wav"))?;
        let annotations =
            load_annotations(&data_dir.join("fsl10k-annotations").join("annotations"))?;

        let mut sounds = Vec::with_capacity(metadata.len());
        let mut missing_audio = Vec::new();
        for (id_text, entry) in metadata {
            let id: u64 = id_text
                .parse()
                .map_err(|error| format!("metadata id {id_text:?} is not a number: {error}"))?;
            let Some(audio_path) = audio.get(&id) else {
                missing_audio.push(id);
                continue;
            };
            let expert = annotations.get(&id).map(Vec::as_slice).unwrap_or_default();
            let genres = merge_genres(parent_genres.get(&id_text), expert);
            sounds.push(Sound {
                id,
                split: Split::Train,
                username: entry.username,
                audio_path: audio_path.clone(),
                labels: resolve_labels(entry.annotations.bpm, expert),
                is_hip_hop: genres.contains(HIP_HOP_GENRE),
                genres,
            });
        }
        let splits: HashMap<String, Split> =
            assign_splits(sounds.iter().map(|sound| sound.username.as_str()))
                .into_iter()
                .map(|(name, split)| (name.to_string(), split))
                .collect();
        for sound in &mut sounds {
            sound.split = splits[sound.username.as_str()];
        }
        sounds.sort_by_key(|sound| sound.id);
        missing_audio.sort_unstable();
        Ok(Self {
            sounds,
            missing_audio,
        })
    }
}

/// Turns raw labels into the ground truth every metric is scored against.
///
/// - Annotations with `discard: true` are ignored; if all are discarded the sound has no expert labels.
/// - `tempo_creator` is the metadata BPM when positive.
/// - `tempo_expert` needs every remaining annotator to give a BPM, all within `TEMPO_TOLERANCE`
///   of each other (their mean is used); otherwise it is `Ambiguous`.
/// - `key` is `Atonal` when every annotator answered key `none` or mode `none`, `Key` when every
///   annotator gave the same real key with mode `maj`/`min`, and `Ambiguous` for anything else
///   (disagreement, `unknown`, or a mix of keyed and keyless answers).
pub fn resolve_labels(creator_bpm: f32, annotations: &[ExpertAnnotation]) -> Labels {
    let kept: Vec<&ExpertAnnotation> = annotations.iter().filter(|a| !a.discard).collect();
    Labels {
        tempo_creator: (creator_bpm > 0.0).then_some(creator_bpm),
        tempo_expert: resolve_expert_tempo(&kept),
        key: resolve_key(&kept),
    }
}

fn resolve_expert_tempo(annotations: &[&ExpertAnnotation]) -> ExpertTempo {
    if annotations.is_empty() {
        return ExpertTempo::Unlabeled;
    }
    let parsed: Option<Vec<f32>> = annotations
        .iter()
        .map(|a| a.bpm.trim().parse::<f32>().ok().filter(|bpm| *bpm > 0.0))
        .collect();
    let Some(bpms) = parsed else {
        return ExpertTempo::Ambiguous;
    };
    let slowest = bpms.iter().copied().fold(f32::INFINITY, f32::min);
    let fastest = bpms.iter().copied().fold(0.0, f32::max);
    if fastest > slowest * (1.0 + TEMPO_TOLERANCE) {
        return ExpertTempo::Ambiguous;
    }
    ExpertTempo::Bpm(bpms.iter().sum::<f32>() / bpms.len() as f32)
}

/// What a single annotator said about the key.
#[derive(PartialEq)]
enum KeyVote {
    Atonal,
    Key(Key),
    Unclear,
}

fn resolve_key(annotations: &[&ExpertAnnotation]) -> KeyLabel {
    let Some(first) = annotations.first() else {
        return KeyLabel::Unlabeled;
    };
    let first_vote = key_vote(first);
    let is_unanimous = annotations.iter().all(|a| key_vote(a) == first_vote);
    match first_vote {
        KeyVote::Atonal if is_unanimous => KeyLabel::Atonal,
        KeyVote::Key(key) if is_unanimous => KeyLabel::Key(key),
        _ => KeyLabel::Ambiguous,
    }
}

fn key_vote(annotation: &ExpertAnnotation) -> KeyVote {
    const NONE: &str = "none";
    let key = annotation.key.trim().to_ascii_lowercase();
    let mode = annotation.mode.trim().to_ascii_lowercase();
    if key == NONE || mode == NONE {
        return KeyVote::Atonal;
    }
    let mode = match mode.as_str() {
        "maj" => Mode::Major,
        "min" => Mode::Minor,
        _ => return KeyVote::Unclear,
    };
    match parse_pitch_class(&key) {
        Some(tonic) => KeyVote::Key(Key { tonic, mode }),
        None => KeyVote::Unclear,
    }
}

/// Parses a lowercase note name like `c`, `f#` or `bb`.
pub fn parse_pitch_class(name: &str) -> Option<PitchClass> {
    const NATURALS: [(char, i8); 7] = [
        ('c', 0),
        ('d', 2),
        ('e', 4),
        ('f', 5),
        ('g', 7),
        ('a', 9),
        ('b', 11),
    ];
    let mut chars = name.chars();
    let letter = chars.next()?;
    let natural = NATURALS.iter().find(|(c, _)| *c == letter)?.1;
    let accidental = match chars.as_str() {
        "" => 0,
        "#" => 1,
        "b" => -1,
        _ => return None,
    };
    let index = (natural + accidental).rem_euclid(12);
    PitchClass::new(u8::try_from(index).ok()?)
}

fn merge_genres(
    parent_genres: Option<&Vec<String>>,
    annotations: &[ExpertAnnotation],
) -> BTreeSet<String> {
    let expert = annotations
        .iter()
        .filter(|a| !a.discard)
        .flat_map(|a| &a.genres);
    parent_genres
        .into_iter()
        .flatten()
        .chain(expert)
        .map(|genre| genre.trim().to_lowercase())
        .collect()
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    serde_json::from_str(&text).map_err(|error| format!("cannot parse {}: {error}", path.display()))
}

/// Maps sound id to its audio file. Files are named `<id>_<userid>.<ext>.wav`.
fn index_audio(dir: &Path) -> Result<HashMap<u64, PathBuf>, String> {
    let entries =
        fs::read_dir(dir).map_err(|error| format!("cannot list {}: {error}", dir.display()))?;
    let mut audio = HashMap::new();
    for entry in entries {
        let path = entry
            .map_err(|error| format!("cannot list {}: {error}", dir.display()))?
            .path();
        let Some(id) = leading_id(&path) else {
            continue;
        };
        audio.insert(id, path);
    }
    Ok(audio)
}

fn leading_id(path: &Path) -> Option<u64> {
    let name = path.file_name()?.to_str()?;
    let (id, _) = name.split_once('_')?;
    id.parse().ok()
}

/// Reads `<annotator>/sound-<id>.json` under `dir`, grouping every annotator's judgment by sound id.
fn load_annotations(dir: &Path) -> Result<BTreeMap<u64, Vec<ExpertAnnotation>>, String> {
    let annotators =
        fs::read_dir(dir).map_err(|error| format!("cannot list {}: {error}", dir.display()))?;
    let mut annotations: BTreeMap<u64, Vec<ExpertAnnotation>> = BTreeMap::new();
    for annotator in annotators {
        let annotator = annotator
            .map_err(|error| format!("cannot list {}: {error}", dir.display()))?
            .path();
        // The archive ships stray files like `.DS_Store` next to the annotator folders.
        if !annotator.is_dir() {
            continue;
        }
        let files = fs::read_dir(&annotator)
            .map_err(|error| format!("cannot list {}: {error}", annotator.display()))?;
        for file in files {
            let path = file
                .map_err(|error| format!("cannot list {}: {error}", annotator.display()))?
                .path();
            let Some(id) = annotation_id(&path) else {
                continue;
            };
            annotations.entry(id).or_default().push(read_json(&path)?);
        }
    }
    Ok(annotations)
}

fn annotation_id(path: &Path) -> Option<u64> {
    let stem = path.file_name()?.to_str()?.strip_suffix(".json")?;
    stem.strip_prefix("sound-")?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn annotation(bpm: &str, key: &str, mode: &str) -> ExpertAnnotation {
        ExpertAnnotation {
            bpm: bpm.to_string(),
            key: key.to_string(),
            mode: mode.to_string(),
            genres: Vec::new(),
            discard: false,
        }
    }

    fn discarded(bpm: &str, key: &str, mode: &str) -> ExpertAnnotation {
        ExpertAnnotation {
            discard: true,
            ..annotation(bpm, key, mode)
        }
    }

    fn key(index: u8, mode: Mode) -> Key {
        let tonic = PitchClass::new(index).expect("valid pitch class");
        Key { tonic, mode }
    }

    #[test]
    fn creator_tempo_needs_a_positive_bpm() {
        assert_eq!(resolve_labels(140.0, &[]).tempo_creator, Some(140.0));
        assert_eq!(resolve_labels(0.0, &[]).tempo_creator, None);
    }

    #[test]
    fn no_annotations_means_unlabeled() {
        let labels = resolve_labels(90.0, &[]);
        assert_eq!(labels.tempo_expert, ExpertTempo::Unlabeled);
        assert_eq!(labels.key, KeyLabel::Unlabeled);
    }

    #[test]
    fn all_discarded_means_unlabeled() {
        let labels = resolve_labels(90.0, &[discarded("90", "c", "maj")]);
        assert_eq!(labels.tempo_expert, ExpertTempo::Unlabeled);
        assert_eq!(labels.key, KeyLabel::Unlabeled);
    }

    #[test]
    fn discarded_annotations_do_not_vote() {
        let labels = resolve_labels(
            90.0,
            &[annotation("90", "c", "maj"), discarded("140", "d", "min")],
        );
        assert_eq!(labels.tempo_expert, ExpertTempo::Bpm(90.0));
        assert_eq!(labels.key, KeyLabel::Key(key(0, Mode::Major)));
    }

    #[test]
    fn agreeing_expert_tempi_are_averaged() {
        let labels = resolve_labels(0.0, &[annotation("100", "", ""), annotation("102", "", "")]);
        assert_eq!(labels.tempo_expert, ExpertTempo::Bpm(101.0));
    }

    #[test]
    fn disagreeing_or_missing_expert_tempi_are_ambiguous() {
        let octave_apart =
            resolve_labels(0.0, &[annotation("70", "", ""), annotation("140", "", "")]);
        assert_eq!(octave_apart.tempo_expert, ExpertTempo::Ambiguous);
        let one_missing =
            resolve_labels(0.0, &[annotation("70", "", ""), annotation("none", "", "")]);
        assert_eq!(one_missing.tempo_expert, ExpertTempo::Ambiguous);
    }

    #[test]
    fn keyless_answers_mean_atonal() {
        let labels = resolve_labels(
            0.0,
            &[
                annotation("90", "none", "none"),
                annotation("90", "c", "none"),
            ],
        );
        assert_eq!(labels.key, KeyLabel::Atonal);
    }

    #[test]
    fn agreeing_keys_are_labeled() {
        let labels = resolve_labels(
            0.0,
            &[annotation("90", "f#", "min"), annotation("90", "f#", "min")],
        );
        assert_eq!(labels.key, KeyLabel::Key(key(6, Mode::Minor)));
    }

    #[test]
    fn unclear_or_conflicting_keys_are_ambiguous() {
        let cases = [
            vec![annotation("90", "c", "maj"), annotation("90", "a", "min")],
            vec![annotation("90", "unknown", "maj")],
            vec![annotation("90", "c", "unknown")],
            vec![
                annotation("90", "c", "maj"),
                annotation("90", "none", "none"),
            ],
        ];
        for annotations in cases {
            assert_eq!(resolve_labels(0.0, &annotations).key, KeyLabel::Ambiguous);
        }
    }

    #[test]
    fn parses_sharps_and_flats() {
        assert_eq!(parse_pitch_class("c").map(PitchClass::index), Some(0));
        assert_eq!(parse_pitch_class("g#").map(PitchClass::index), Some(8));
        assert_eq!(parse_pitch_class("bb").map(PitchClass::index), Some(10));
        assert_eq!(parse_pitch_class("cb").map(PitchClass::index), Some(11));
        assert_eq!(parse_pitch_class("h"), None);
    }

    #[test]
    fn fnv1a_matches_reference_values() {
        assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
    }

    #[test]
    fn splits_keep_creators_whole_and_balance_sizes() {
        // One dominant creator plus many small ones, like FSL10K.
        let mut names: Vec<String> = vec!["big".to_string(); 300];
        names.extend((0..700).map(|i| format!("creator{}", i % 140)));
        let splits = assign_splits(names.iter().map(String::as_str));

        assert_eq!(splits["big"], Split::Train);
        let mut counts = [0usize; 3];
        for name in &names {
            counts[Split::ALL
                .iter()
                .position(|s| *s == splits[name.as_str()])
                .unwrap()] += 1;
        }
        assert!((680..=720).contains(&counts[0]), "{counts:?}");
        assert!((130..=170).contains(&counts[1]), "{counts:?}");
        assert!((130..=170).contains(&counts[2]), "{counts:?}");
        assert_eq!(assign_splits(names.iter().map(String::as_str)), splits);
    }

    #[test]
    fn genres_merge_metadata_and_kept_annotations() {
        let parent = vec!["hip hop".to_string()];
        let mut kept = annotation("90", "c", "maj");
        kept.genres = vec!["Electronic".to_string()];
        let mut dropped = discarded("90", "c", "maj");
        dropped.genres = vec!["cinematic".to_string()];

        let genres = merge_genres(Some(&parent), &[kept, dropped]);
        let expected: BTreeSet<String> = ["electronic", "hip hop"].map(String::from).into();
        assert_eq!(genres, expected);
    }

    #[test]
    fn reads_ids_from_file_names() {
        assert_eq!(
            leading_id(Path::new("wav/100269_1676089.wav.wav")),
            Some(100_269)
        );
        assert_eq!(leading_id(Path::new("wav/.DS_Store")), None);
        assert_eq!(
            annotation_id(Path::new("1/sound-100902.json")),
            Some(100_902)
        );
        assert_eq!(annotation_id(Path::new("1/notes.txt")), None);
    }
}
