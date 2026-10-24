//! Key profiles and correlation scoring over the 24 major/minor keys.

use super::PITCH_CLASSES;
use crate::{Key, Mode, PitchClass};

type Profile = [f32; PITCH_CLASSES];

/// Krumhansl & Kessler (1982) probe-tone ratings.
const KRUMHANSL_KESSLER_MAJOR: Profile = [
    6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88,
];
const KRUMHANSL_KESSLER_MINOR: Profile = [
    6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17,
];
/// Temperley (1999), "What's key for key?".
const TEMPERLEY_MAJOR: Profile = [5.0, 2.0, 3.5, 2.0, 4.5, 4.0, 2.0, 4.5, 2.0, 3.5, 1.5, 4.0];
const TEMPERLEY_MINOR: Profile = [5.0, 2.0, 3.5, 4.5, 2.0, 4.0, 2.0, 4.5, 3.5, 2.0, 1.5, 4.0];
/// Faraldo et al. (2016), "edma" profiles derived from electronic dance music.
const EDMA_MAJOR: Profile = [
    1.00, 0.29, 0.50, 0.40, 0.60, 0.56, 0.32, 0.80, 0.31, 0.45, 0.42, 0.39,
];
const EDMA_MINOR: Profile = [
    1.00, 0.31, 0.44, 0.58, 0.33, 0.49, 0.29, 0.78, 0.43, 0.29, 0.53, 0.32,
];
/// Sha'ath (2011), KeyFinder's profiles tuned on popular and electronic music.
const SHAATH_MAJOR: Profile = [6.6, 2.0, 3.5, 2.3, 4.6, 4.0, 2.5, 5.2, 2.4, 3.7, 2.3, 3.4];
const SHAATH_MINOR: Profile = [6.5, 2.7, 3.5, 5.4, 2.6, 3.5, 2.5, 5.2, 4.0, 2.7, 4.3, 3.2];

/// Semitones up from a major tonic to its relative minor's tonic, and back.
const RELATIVE_MINOR_OFFSET: u8 = 9;
const RELATIVE_MAJOR_OFFSET: u8 = 3;
const MODES: [Mode; 2] = [Mode::Major, Mode::Minor];
const KEY_COUNT: usize = MODES.len() * PITCH_CLASSES;

/// Which major/minor templates the chroma is correlated against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KeyProfile {
    KrumhanslKessler,
    Temperley,
    /// Faraldo's electronic dance music profile.
    #[default]
    Edma,
    /// Sha'ath's KeyFinder profile.
    Shaath,
}

impl KeyProfile {
    pub const ALL: [KeyProfile; 4] = [
        KeyProfile::KrumhanslKessler,
        KeyProfile::Temperley,
        KeyProfile::Edma,
        KeyProfile::Shaath,
    ];

    fn template(self, mode: Mode) -> &'static Profile {
        match (self, mode) {
            (KeyProfile::KrumhanslKessler, Mode::Major) => &KRUMHANSL_KESSLER_MAJOR,
            (KeyProfile::KrumhanslKessler, Mode::Minor) => &KRUMHANSL_KESSLER_MINOR,
            (KeyProfile::Temperley, Mode::Major) => &TEMPERLEY_MAJOR,
            (KeyProfile::Temperley, Mode::Minor) => &TEMPERLEY_MINOR,
            (KeyProfile::Edma, Mode::Major) => &EDMA_MAJOR,
            (KeyProfile::Edma, Mode::Minor) => &EDMA_MINOR,
            (KeyProfile::Shaath, Mode::Major) => &SHAATH_MAJOR,
            (KeyProfile::Shaath, Mode::Minor) => &SHAATH_MINOR,
        }
    }
}

/// The best key and how clearly it beats keys with a different scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct KeyRanking {
    pub best: Key,
    /// Pearson correlation of the best key plus its tonic cue.
    pub best_score: f32,
    /// Best correlation among keys that are neither `best` nor its relative.
    pub rival_score: f32,
}

/// How keys are scored against a chroma.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct KeyScoring {
    pub profile: KeyProfile,
    /// Per pitch class, 0..=1 evidence that it is the tonic (the bass note distribution).
    pub tonic_cue: [f32; PITCH_CLASSES],
    /// How much `tonic_cue` at a key's tonic adds to that key's correlation.
    pub tonic_weight: f32,
}

/// Correlates `chroma` with every key of the profile. `None` when the chroma is flat.
pub(super) fn rank_keys(chroma: &[f32; PITCH_CLASSES], scoring: KeyScoring) -> Option<KeyRanking> {
    let centered_chroma = centered_unit(chroma)?;
    let mut scores = [0.0f32; KEY_COUNT];
    for (mode_index, mode) in MODES.into_iter().enumerate() {
        let template = centered_unit(scoring.profile.template(mode))
            .expect("built-in key profiles are not flat");
        for tonic in 0..PITCH_CLASSES {
            let correlation: f32 = (0..PITCH_CLASSES)
                .map(|pitch| centered_chroma[(tonic + pitch) % PITCH_CLASSES] * template[pitch])
                .sum();
            scores[mode_index * PITCH_CLASSES + tonic] =
                correlation + scoring.tonic_weight * scoring.tonic_cue[tonic];
        }
    }

    let best_index = (0..KEY_COUNT).max_by(|&a, &b| scores[a].total_cmp(&scores[b]))?;
    let best = key_at(best_index);
    let relative = relative_key(best);
    let rival_score = (0..KEY_COUNT)
        .filter(|&index| index != best_index && key_at(index) != relative)
        .map(|index| scores[index])
        .fold(f32::NEG_INFINITY, f32::max);
    let best_score = scores[best_index];
    Some(KeyRanking {
        best,
        best_score,
        rival_score,
    })
}

/// The key with the same notes and the other mode (C major ↔ A minor).
pub(super) fn relative_key(key: Key) -> Key {
    let (offset, mode) = match key.mode {
        Mode::Major => (RELATIVE_MINOR_OFFSET, Mode::Minor),
        Mode::Minor => (RELATIVE_MAJOR_OFFSET, Mode::Major),
    };
    let tonic = (key.tonic.index() + offset) % PITCH_CLASSES as u8;
    self::key(usize::from(tonic), mode)
}

/// Inverse of the score layout in `rank_keys`: majors first, then minors, by tonic.
fn key_at(index: usize) -> Key {
    key(index % PITCH_CLASSES, MODES[index / PITCH_CLASSES])
}

fn key(tonic: usize, mode: Mode) -> Key {
    let tonic = PitchClass::new((tonic % PITCH_CLASSES) as u8)
        .expect("tonic reduced modulo 12 is a valid pitch class");
    Key { tonic, mode }
}

/// Zero-mean, unit-length copy of `values`, or `None` if they are all equal.
fn centered_unit(values: &[f32; PITCH_CLASSES]) -> Option<[f32; PITCH_CLASSES]> {
    let mean = values.iter().sum::<f32>() / PITCH_CLASSES as f32;
    let mut centered = values.map(|value| value - mean);
    let norm = centered
        .iter()
        .map(|value| value * value)
        .sum::<f32>()
        .sqrt();
    if !norm.is_finite() || norm <= f32::EPSILON {
        return None;
    }
    centered.iter_mut().for_each(|value| *value /= norm);
    Some(centered)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pitch classes of a scale, rotated to `tonic`, each weighted by the profile.
    fn rotated(profile: &Profile, tonic: usize) -> [f32; PITCH_CLASSES] {
        let mut chroma = [0.0; PITCH_CLASSES];
        for (pitch, weight) in profile.iter().enumerate() {
            chroma[(tonic + pitch) % PITCH_CLASSES] = *weight;
        }
        chroma
    }

    #[test]
    fn each_profile_recognizes_its_own_templates() {
        for profile in KeyProfile::ALL {
            for mode in MODES {
                for tonic in 0..PITCH_CLASSES {
                    let chroma = rotated(profile.template(mode), tonic);
                    let scoring = KeyScoring {
                        profile,
                        tonic_cue: [0.0; PITCH_CLASSES],
                        tonic_weight: 0.0,
                    };
                    let ranking = rank_keys(&chroma, scoring).expect("not flat");
                    assert_eq!(ranking.best, key(tonic, mode), "{profile:?}");
                    assert!((ranking.best_score - 1.0).abs() < 1e-4);
                    assert!(ranking.rival_score < ranking.best_score);
                }
            }
        }
    }

    #[test]
    fn flat_chroma_has_no_ranking() {
        let scoring = KeyScoring {
            profile: KeyProfile::Edma,
            tonic_cue: [0.0; PITCH_CLASSES],
            tonic_weight: 0.0,
        };
        assert_eq!(rank_keys(&[1.0; PITCH_CLASSES], scoring), None);
        assert_eq!(rank_keys(&[0.0; PITCH_CLASSES], scoring), None);
    }

    #[test]
    fn tonic_cue_chooses_between_keys_with_the_same_notes() {
        let mut white_keys = [0.0; PITCH_CLASSES];
        for pitch in [0, 2, 4, 5, 7, 9, 11] {
            white_keys[pitch] = 1.0;
        }
        for (bass_note, expected) in [(0, key(0, Mode::Major)), (9, key(9, Mode::Minor))] {
            let mut tonic_cue = [0.0; PITCH_CLASSES];
            tonic_cue[bass_note] = 1.0;
            let scoring = KeyScoring {
                profile: KeyProfile::Edma,
                tonic_cue,
                tonic_weight: 0.1,
            };
            let ranking = rank_keys(&white_keys, scoring).expect("not flat");
            assert_eq!(ranking.best, expected, "bass on {bass_note}");
        }
    }

    #[test]
    fn relative_keys_share_notes() {
        assert_eq!(relative_key(key(0, Mode::Major)), key(9, Mode::Minor));
        assert_eq!(relative_key(key(9, Mode::Minor)), key(0, Mode::Major));
        assert_eq!(relative_key(key(6, Mode::Major)), key(3, Mode::Minor));
    }
}
