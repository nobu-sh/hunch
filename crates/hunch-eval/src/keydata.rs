//! Track-level key datasets labeled with Beatport-style key strings: Beatport EDM Key and
//! GiantSteps+ EDM Key.
//!
//! Both ship `audio/<name>.mp3` with a one-line label in `<labels dir>/<name>.txt`, where `<name>`
//! is `<id> <Artist> - <Title>`. See [`parse_key_label`] for the label rules.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use hunch_core::{Key, Mode};

use crate::fsl10k::{self, Split};

pub const BEATPORT_DIR: &str = "beatport-key";
pub const GIANTSTEPS_DIR: &str = "giantsteps-key";
const BEATPORT_LABELS_DIR: &str = "keys";
const GIANTSTEPS_LABELS_DIR: &str = "keys_gs+";
const AUDIO_DIR: &str = "audio";
const AUDIO_EXTENSION: &str = "mp3";
const LABEL_EXTENSION: &str = "txt";
/// Beatport is split by artist into train and val only; GiantSteps+ is the test set.
const BEATPORT_SPLIT_PERCENTS: [(Split, u64); 2] = [(Split::Train, 80), (Split::Val, 20)];
/// Tuning flags some annotators appended to the tonic (`G#_`, `A^`).
const TUNING_FLAGS: [char; 2] = ['_', '^'];
const ALTERNATIVE_SEPARATOR: char = '|';
const NO_LABEL: [&str; 2] = ["x", "unknown"];
const OTHER_TONALITY: &str = "other";
/// Church modes and minor variants, folded onto major/minor. Includes the typos found in the
/// label files (`phryrgian`, `phygian`, `aeonian`).
const MINOR_MODE_NAMES: [&str; 9] = [
    "aeolian",
    "aeonian",
    "dorian",
    "phrygian",
    "phryrgian",
    "phygian",
    "harmonic",
    "melodic",
    "minor",
];
const MAJOR_MODE_NAMES: [&str; 6] = [
    "ionian",
    "mixolydian",
    "mixolydian-b6",
    "lydian",
    "phrygian-major",
    "major",
];

/// Why a label can't be scored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Exclusion {
    /// `X`, `unknown` or an empty file: the annotator gave no key.
    NoLabel,
    /// Every alternative is `other` (modal or atonal music outside major/minor).
    Other,
    /// The major/minor word and the mode name disagree (`B major aeolian`).
    ModeConflict,
    /// Text the rules below don't cover.
    Unparsed,
    /// The audio file has no label file.
    Missing,
}

impl Exclusion {
    pub fn name(self) -> &'static str {
        match self {
            Exclusion::NoLabel => "no label (X/unknown)",
            Exclusion::Other => "other tonality",
            Exclusion::ModeConflict => "mode conflict",
            Exclusion::Unparsed => "unparsed",
            Exclusion::Missing => "missing label file",
        }
    }
}

/// Parses a key label into the keys that count as correct.
///
/// - `_` and `^` (tuning flags) are stripped and case is ignored.
/// - `X`, `unknown` or nothing means no label.
/// - Alternatives are separated by `|`; any of them counts as correct.
/// - Each alternative is `<tonic> <major|minor|other> [mode name]`, with sharps or flats.
///   Mode names fold onto major/minor: aeolian, dorian, phrygian, harmonic and melodic are minor;
///   ionian, mixolydian, lydian and phrygian-major (also written `phrygian major`) are major.
///   An alternative whose mode name contradicts its major/minor word is dropped.
/// - `other` alternatives are dropped (e.g. `Bb other | A minor` keeps only A minor); a label with
///   nothing left is excluded.
pub fn parse_key_label(text: &str) -> Result<Vec<Key>, Exclusion> {
    let cleaned: String = text
        .chars()
        .filter(|c| !TUNING_FLAGS.contains(c))
        .collect::<String>()
        .to_lowercase();
    let cleaned = cleaned.trim();
    if cleaned.is_empty() || NO_LABEL.contains(&cleaned) {
        return Err(Exclusion::NoLabel);
    }

    let mut keys = Vec::new();
    let mut has_conflict = false;
    for alternative in cleaned.split(ALTERNATIVE_SEPARATOR) {
        match parse_alternative(alternative)? {
            Alternative::Key(key) if !keys.contains(&key) => keys.push(key),
            Alternative::Key(_) | Alternative::Other => {}
            Alternative::Conflict => has_conflict = true,
        }
    }
    match (keys.is_empty(), has_conflict) {
        (false, _) => Ok(keys),
        (true, true) => Err(Exclusion::ModeConflict),
        (true, false) => Err(Exclusion::Other),
    }
}

enum Alternative {
    Key(Key),
    Other,
    Conflict,
}

fn parse_alternative(text: &str) -> Result<Alternative, Exclusion> {
    let mut words = text.split_whitespace();
    let tonic = words
        .next()
        .and_then(fsl10k::parse_pitch_class)
        .ok_or(Exclusion::Unparsed)?;
    let mode_word = words.next().ok_or(Exclusion::Unparsed)?;
    if mode_word == OTHER_TONALITY {
        return Ok(Alternative::Other);
    }
    let mode = mode_of(mode_word).ok_or(Exclusion::Unparsed)?;
    let mode_name = words.collect::<Vec<_>>().join("-");
    if !mode_name.is_empty() && mode_of(&mode_name).ok_or(Exclusion::Unparsed)? != mode {
        return Ok(Alternative::Conflict);
    }
    Ok(Alternative::Key(Key { tonic, mode }))
}

fn mode_of(name: &str) -> Option<Mode> {
    if MINOR_MODE_NAMES.contains(&name) {
        Some(Mode::Minor)
    } else if MAJOR_MODE_NAMES.contains(&name) {
        Some(Mode::Major)
    } else {
        None
    }
}

/// The first artist of a `<id> <Artist[, Artist…]> - <Title>` file stem, lowercased with
/// whitespace collapsed, so the same artist groups together across tracks.
pub fn first_artist(stem: &str) -> Option<String> {
    let (_, rest) = stem.split_once(' ')?;
    let (artists, _) = rest.split_once(" - ")?;
    let first = artists.split(',').next()?;
    let normalized = first.split_whitespace().collect::<Vec<_>>().join(" ");
    (!normalized.is_empty()).then(|| normalized.to_lowercase())
}

fn leading_id(stem: &str) -> Option<u64> {
    stem.split_once(' ')?.0.parse().ok()
}

#[derive(Debug, Clone)]
pub struct Track {
    pub id: u64,
    pub artist: String,
    pub split: Split,
    pub audio_path: PathBuf,
    /// Keys that count as correct, or why the track is excluded.
    pub label: Result<Vec<Key>, Exclusion>,
}

/// Every track with audio, sorted by id.
pub struct KeyDataset {
    pub name: &'static str,
    pub tracks: Vec<Track>,
}

impl KeyDataset {
    /// Beatport EDM Key from `data_dir/beatport-key`, split 80/20 into train/val by first artist.
    pub fn load_beatport(data_dir: &Path) -> Result<Self, String> {
        let root = data_dir.join(BEATPORT_DIR);
        let mut tracks = load_tracks(&root, BEATPORT_LABELS_DIR)?;
        let splits = fsl10k::assign_groups(
            tracks.iter().map(|track| track.artist.as_str()),
            &BEATPORT_SPLIT_PERCENTS,
        );
        let splits: BTreeMap<String, Split> = splits
            .into_iter()
            .map(|(artist, split)| (artist.to_string(), split))
            .collect();
        for track in &mut tracks {
            track.split = splits[&track.artist];
        }
        Ok(Self {
            name: "beatport",
            tracks,
        })
    }

    /// GiantSteps+ EDM Key from `data_dir/giantsteps-key`, all in the test split.
    pub fn load_giantsteps(data_dir: &Path) -> Result<Self, String> {
        let root = data_dir.join(GIANTSTEPS_DIR);
        Ok(Self {
            name: "giantsteps",
            tracks: load_tracks(&root, GIANTSTEPS_LABELS_DIR)?,
        })
    }
}

/// Reads every `audio/*.mp3` under `root` with its label from `root/<labels_dir>`. Tracks start
/// in the test split.
fn load_tracks(root: &Path, labels_dir: &str) -> Result<Vec<Track>, String> {
    let audio_dir = root.join(AUDIO_DIR);
    let entries = fs::read_dir(&audio_dir)
        .map_err(|error| format!("cannot list {}: {error}", audio_dir.display()))?;
    let mut tracks = Vec::new();
    for entry in entries {
        let audio_path = entry
            .map_err(|error| format!("cannot list {}: {error}", audio_dir.display()))?
            .path();
        let is_audio = audio_path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case(AUDIO_EXTENSION));
        let Some(stem) = audio_path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        if !is_audio {
            continue;
        }
        let (Some(id), Some(artist)) = (leading_id(stem), first_artist(stem)) else {
            return Err(format!(
                "{} is not named `<id> <Artist> - <Title>`",
                audio_path.display()
            ));
        };
        let label_path = root
            .join(labels_dir)
            .join(format!("{stem}.{LABEL_EXTENSION}"));
        let label = match fs::read_to_string(&label_path) {
            Ok(text) => parse_key_label(&text),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(Exclusion::Missing),
            Err(error) => return Err(format!("cannot read {}: {error}", label_path.display())),
        };
        tracks.push(Track {
            id,
            artist,
            split: Split::Test,
            audio_path,
            label,
        });
    }
    tracks.sort_by_key(|track| track.id);
    Ok(tracks)
}

/// Label counts for one split.
#[derive(Debug, Default)]
pub struct TrackLabelStats {
    pub tracks: usize,
    pub usable: usize,
    pub major: usize,
    pub minor: usize,
    /// Usable labels with more than one acceptable key.
    pub with_alternatives: usize,
    pub excluded: BTreeMap<Exclusion, usize>,
    pub artists: usize,
}

impl TrackLabelStats {
    /// Counts by the first listed key's mode.
    pub fn count<'a>(tracks: impl IntoIterator<Item = &'a Track>) -> Self {
        let mut stats = Self::default();
        let mut artists = std::collections::BTreeSet::new();
        for track in tracks {
            stats.tracks += 1;
            artists.insert(track.artist.as_str());
            match &track.label {
                Ok(keys) => {
                    stats.usable += 1;
                    stats.with_alternatives += usize::from(keys.len() > 1);
                    match keys.first().map(|key| key.mode) {
                        Some(Mode::Major) => stats.major += 1,
                        Some(Mode::Minor) => stats.minor += 1,
                        None => {}
                    }
                }
                Err(exclusion) => *stats.excluded.entry(*exclusion).or_default() += 1,
            }
        }
        stats.artists = artists.len();
        stats
    }
}

pub fn print_label_stats(dataset: &KeyDataset) {
    println!("{} label stats:", dataset.name);
    let all = TrackLabelStats::count(&dataset.tracks);
    let mut rows = vec![("all".to_string(), all)];
    for split in Split::ALL {
        let tracks = dataset.tracks.iter().filter(|track| track.split == split);
        let stats = TrackLabelStats::count(tracks);
        if stats.tracks > 0 {
            rows.push((split.to_string(), stats));
        }
    }
    for (name, stats) in rows {
        let excluded: Vec<String> = stats
            .excluded
            .iter()
            .map(|(exclusion, count)| format!("{} {count}", exclusion.name()))
            .collect();
        println!(
            "  {name:<5} tracks {:>4} ({} artists)  usable {:>4} (major {}, minor {}, with alternatives {})  excluded: {}",
            stats.tracks,
            stats.artists,
            stats.usable,
            stats.major,
            stats.minor,
            stats.with_alternatives,
            if excluded.is_empty() {
                "none".to_string()
            } else {
                excluded.join(", ")
            }
        );
    }
}

#[cfg(test)]
mod tests {
    use hunch_core::PitchClass;

    use super::*;

    fn key(index: u8, mode: Mode) -> Key {
        let tonic = PitchClass::new(index).expect("valid pitch class");
        Key { tonic, mode }
    }

    #[test]
    fn parses_plain_keys_with_sharps_and_flats() {
        assert_eq!(parse_key_label("D minor"), Ok(vec![key(2, Mode::Minor)]));
        assert_eq!(parse_key_label("E major"), Ok(vec![key(4, Mode::Major)]));
        assert_eq!(parse_key_label("Bb major"), Ok(vec![key(10, Mode::Major)]));
        assert_eq!(parse_key_label("A# minor"), Ok(vec![key(10, Mode::Minor)]));
        assert_eq!(parse_key_label("Gb minor "), Ok(vec![key(6, Mode::Minor)]));
    }

    #[test]
    fn folds_mode_names_onto_major_and_minor() {
        let cases = [
            ("F minor phrygian", key(5, Mode::Minor)),
            ("C minor aeolian", key(0, Mode::Minor)),
            ("E minor dorian", key(4, Mode::Minor)),
            ("Gb minor harmonic", key(6, Mode::Minor)),
            ("E minor melodic", key(4, Mode::Minor)),
            ("F minor phryrgian", key(5, Mode::Minor)),
            ("G major mixolydian", key(7, Mode::Major)),
            ("G# major mixolydian-b6", key(8, Mode::Major)),
            ("F major lydian", key(5, Mode::Major)),
            ("C major ionian", key(0, Mode::Major)),
            ("D major phrygian-major", key(2, Mode::Major)),
        ];
        for (text, expected) in cases {
            assert_eq!(parse_key_label(text), Ok(vec![expected]), "{text}");
        }
    }

    #[test]
    fn any_alternative_counts() {
        assert_eq!(
            parse_key_label("C major | A minor"),
            Ok(vec![key(0, Mode::Major), key(9, Mode::Minor)])
        );
        assert_eq!(
            parse_key_label("E major phrygian major| E minor phrygian"),
            Ok(vec![key(4, Mode::Major), key(4, Mode::Minor)])
        );
        assert_eq!(
            parse_key_label("Db minor aeolian |A major"),
            Ok(vec![key(1, Mode::Minor), key(9, Mode::Major)])
        );
    }

    #[test]
    fn strips_tuning_flags() {
        assert_eq!(
            parse_key_label("G#_ minor | A^ minor"),
            Ok(vec![key(8, Mode::Minor), key(9, Mode::Minor)])
        );
        assert_eq!(parse_key_label("Eb^ minor"), Ok(vec![key(3, Mode::Minor)]));
    }

    #[test]
    fn excludes_missing_and_other_labels() {
        assert_eq!(parse_key_label("X"), Err(Exclusion::NoLabel));
        assert_eq!(parse_key_label("unknown"), Err(Exclusion::NoLabel));
        assert_eq!(parse_key_label(""), Err(Exclusion::NoLabel));
        assert_eq!(parse_key_label("F other"), Err(Exclusion::Other));
        assert_eq!(parse_key_label("Eb other monotonic"), Err(Exclusion::Other));
        assert_eq!(parse_key_label("G other | C# other"), Err(Exclusion::Other));
    }

    #[test]
    fn other_alternatives_are_dropped() {
        assert_eq!(
            parse_key_label("Bb other | A minor"),
            Ok(vec![key(9, Mode::Minor)])
        );
    }

    #[test]
    fn contradicting_mode_names_are_excluded() {
        assert_eq!(
            parse_key_label("B major aeolian"),
            Err(Exclusion::ModeConflict)
        );
        assert_eq!(
            parse_key_label("Ab minor mixolydian | Ab major dorian"),
            Err(Exclusion::ModeConflict)
        );
    }

    #[test]
    fn rejects_unknown_text() {
        assert_eq!(parse_key_label("H minor"), Err(Exclusion::Unparsed));
        assert_eq!(parse_key_label("C"), Err(Exclusion::Unparsed));
        assert_eq!(parse_key_label("C minor locrian"), Err(Exclusion::Unparsed));
    }

    #[test]
    fn groups_tracks_by_first_artist() {
        assert_eq!(
            first_artist("100066 Lindstrom - Monsteer (Original Mix)").as_deref(),
            Some("lindstrom")
        );
        assert_eq!(
            first_artist("42 D-Lux,  Jonez - Night - Dub Mix").as_deref(),
            Some("d-lux")
        );
        assert_eq!(
            first_artist("7 Gabriel  &  Castellon - Song").as_deref(),
            Some("gabriel & castellon")
        );
        assert_eq!(first_artist("no separator"), None);
        assert_eq!(leading_id("1017875 JT Donaldson - UCFWM"), Some(1_017_875));
    }

    /// Fake data folder under the temp dir holding one dataset; removed on drop.
    struct FakeData {
        data_dir: PathBuf,
        dataset_root: PathBuf,
    }

    impl FakeData {
        fn new(dataset_dir: &str, labels_dir: &str, tracks: &[(&str, Option<&str>)]) -> Self {
            let data_dir = std::env::temp_dir().join(format!("hunch-eval-keydata-{dataset_dir}"));
            fs::remove_dir_all(&data_dir).ok();
            let dataset_root = data_dir.join(dataset_dir);
            fs::create_dir_all(dataset_root.join(AUDIO_DIR)).expect("create audio dir");
            fs::create_dir_all(dataset_root.join(labels_dir)).expect("create labels dir");
            for (stem, label) in tracks {
                fs::write(
                    dataset_root.join(AUDIO_DIR).join(format!("{stem}.mp3")),
                    b"",
                )
                .expect("write audio");
                if let Some(label) = label {
                    fs::write(
                        dataset_root.join(labels_dir).join(format!("{stem}.txt")),
                        label,
                    )
                    .expect("write label");
                }
            }
            Self {
                data_dir,
                dataset_root,
            }
        }
    }

    impl Drop for FakeData {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.data_dir).ok();
        }
    }

    #[test]
    fn loads_tracks_with_labels_and_exclusions() {
        let fake = FakeData::new(
            GIANTSTEPS_DIR,
            GIANTSTEPS_LABELS_DIR,
            &[
                ("20 Artist B - Two", Some("X")),
                ("10 Artist A - One", Some("F minor aeolian")),
                ("30 Artist C - Three", None),
            ],
        );
        let dataset = KeyDataset::load_giantsteps(&fake.data_dir).expect("load");
        type Summary<'a> = (u64, &'a str, Split, Result<Vec<Key>, Exclusion>);
        let summary: Vec<Summary> = dataset
            .tracks
            .iter()
            .map(|t| (t.id, t.artist.as_str(), t.split, t.label.clone()))
            .collect();
        assert_eq!(
            summary,
            vec![
                (10, "artist a", Split::Test, Ok(vec![key(5, Mode::Minor)])),
                (20, "artist b", Split::Test, Err(Exclusion::NoLabel)),
                (30, "artist c", Split::Test, Err(Exclusion::Missing)),
            ]
        );
        let stats = TrackLabelStats::count(&dataset.tracks);
        assert_eq!((stats.tracks, stats.usable, stats.minor), (3, 1, 1));
        assert!(fake.dataset_root.exists());
    }

    #[test]
    fn beatport_splits_keep_artists_whole() {
        let stems: Vec<String> = (0..50)
            .map(|i| format!("{i} Artist{} - Song {i}", i % 10))
            .collect();
        let tracks: Vec<(&str, Option<&str>)> = stems
            .iter()
            .map(|s| (s.as_str(), Some("C minor")))
            .collect();
        let fake = FakeData::new(BEATPORT_DIR, BEATPORT_LABELS_DIR, &tracks);

        let dataset = KeyDataset::load_beatport(&fake.data_dir).expect("load");
        let mut split_of_artist: BTreeMap<&str, Split> = BTreeMap::new();
        for track in &dataset.tracks {
            let split = *split_of_artist
                .entry(track.artist.as_str())
                .or_insert(track.split);
            assert_eq!(split, track.split, "{} straddles splits", track.artist);
        }
        let val = dataset
            .tracks
            .iter()
            .filter(|t| t.split == Split::Val)
            .count();
        assert_eq!(val, 10);
    }
}
