//! Builds synthetic mixes: track A, a transition, then track B.

use std::f32::consts::FRAC_PI_2;

use hunch_core::Key;

use super::features::{flux_of, frame_rate};
use super::pool::{
    Category, MAX_DJ_SEMITONES, PoolTrack, Rng, is_same_key, plain_category, transpose,
};
use crate::decode;
use crate::resample::{self, ANALYSIS_SAMPLE_RATE};

/// Both tracks are gain-matched to this RMS before mixing, like a DJ's trim.
const TARGET_RMS: f32 = 0.1;
/// Track A plays for a random length in this range before the transition starts.
const A_MIN_SECONDS: f32 = 45.0;
const A_MAX_SECONDS: f32 = 75.0;
/// Track B plays this long after the change point.
pub const B_SECONDS: f32 = 60.0;
/// Audio compared when lining B's beats up with A's.
const ALIGN_SECONDS: f32 = 6.0;
const DJ_FADE_SECONDS: [f32; 2] = [8.0, 16.0];

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Transition {
    Cut,
    Linear(f32),
    EqualPower(f32),
    /// B resampled to A's tempo, then a beat-aligned equal-power crossfade.
    Dj(f32),
}

impl Transition {
    pub fn fade_seconds(self) -> f32 {
        match self {
            Transition::Cut => 0.0,
            Transition::Linear(seconds)
            | Transition::EqualPower(seconds)
            | Transition::Dj(seconds) => seconds,
        }
    }

    /// A name that groups like transitions; DJ fades of every length share one.
    pub fn kind(self) -> String {
        match self {
            Transition::Cut => "cut".to_string(),
            Transition::Linear(seconds) => format!("linear {seconds:.0}s"),
            Transition::EqualPower(seconds) => format!("eq-power {seconds:.0}s"),
            Transition::Dj(_) => "dj 8-16s".to_string(),
        }
    }

    /// The plain transitions every pair category is mixed with.
    pub const PLAIN: [Transition; 5] = [
        Transition::Cut,
        Transition::Linear(8.0),
        Transition::EqualPower(4.0),
        Transition::EqualPower(8.0),
        Transition::EqualPower(16.0),
    ];
}

#[derive(Debug, Clone)]
pub struct MixSpec {
    pub id: usize,
    pub a: usize,
    pub b: usize,
    pub a_id: u64,
    pub b_id: u64,
    pub transition: Transition,
    pub category: Category,
    /// Seconds of A before the transition starts.
    pub a_seconds: f32,
    /// Playback speed of B; 1 except for DJ transitions.
    pub b_speed: f32,
    pub a_keys: Vec<Key>,
    /// B's labels, moved by the resampling's pitch shift.
    pub b_keys: Vec<Key>,
    pub a_bpm: Option<f32>,
    pub b_bpm: Option<f32>,
}

pub struct Mix {
    pub samples: Vec<f32>,
    /// The true change point: the crossfade midpoint, or the cut.
    pub change_seconds: f32,
    pub fade_start: f32,
}

/// Picks `per_cell` pairs for every (category, plain transition) cell and for DJ transitions with
/// the same and a different key. Only tracks with a Beatport BPM are used.
pub fn plan_mixes(tracks: &[PoolTrack], per_cell: usize, seed: u64) -> Vec<MixSpec> {
    let mut rng = Rng::new(seed);
    let with_bpm: Vec<usize> = (0..tracks.len())
        .filter(|&i| tracks[i].bpm.is_some())
        .collect();
    let mut pairs: Vec<(usize, usize)> = with_bpm
        .iter()
        .flat_map(|&a| with_bpm.iter().map(move |&b| (a, b)))
        .filter(|(a, b)| a != b)
        .collect();
    rng.shuffle(&mut pairs);

    let mut specs = Vec::new();
    let mut used_pairs = std::collections::HashSet::new();
    for category in Category::ALL {
        // A pair has one category, so categories never share pairs.
        let mut chosen = pairs
            .iter()
            .filter(|&&(a, b)| plain_category(&tracks[a], &tracks[b]) == Some(category));
        for transition in Transition::PLAIN {
            for _ in 0..per_cell {
                let Some(&(a, b)) = chosen.next() else {
                    break;
                };
                used_pairs.insert((a, b));
                specs.push(plain_spec(
                    PlainSpec {
                        tracks,
                        a,
                        b,
                        transition,
                        category,
                    },
                    &mut rng,
                ));
            }
        }
    }

    for wants_same_key in [true, false] {
        let mut added = 0;
        for &(a, b) in &pairs {
            if added == per_cell * 2 {
                break;
            }
            if used_pairs.contains(&(a, b)) {
                continue;
            }
            let Some(spec) = dj_spec(tracks, (a, b), &mut rng) else {
                continue;
            };
            let has_same_key = spec.category == Category::SameKeySameTempo;
            if has_same_key != wants_same_key {
                continue;
            }
            used_pairs.insert((a, b));
            specs.push(spec);
            added += 1;
        }
    }
    for (id, spec) in specs.iter_mut().enumerate() {
        spec.id = id;
    }
    specs
}

struct PlainSpec<'a> {
    tracks: &'a [PoolTrack],
    a: usize,
    b: usize,
    transition: Transition,
    category: Category,
}

fn plain_spec(plain: PlainSpec, rng: &mut Rng) -> MixSpec {
    let (track_a, track_b) = (&plain.tracks[plain.a], &plain.tracks[plain.b]);
    MixSpec {
        id: 0,
        a: plain.a,
        b: plain.b,
        a_id: track_a.id,
        b_id: track_b.id,
        transition: plain.transition,
        category: plain.category,
        a_seconds: A_MIN_SECONDS + rng.unit() * (A_MAX_SECONDS - A_MIN_SECONDS),
        b_speed: 1.0,
        a_keys: track_a.keys.clone(),
        b_keys: track_b.keys.clone(),
        a_bpm: track_a.bpm,
        b_bpm: track_b.bpm,
    }
}

/// A DJ mix of the pair, or `None` when matching the tempo would shift pitch too far.
fn dj_spec(tracks: &[PoolTrack], (a, b): (usize, usize), rng: &mut Rng) -> Option<MixSpec> {
    let (track_a, track_b) = (&tracks[a], &tracks[b]);
    let speed = track_a.bpm? / track_b.bpm?;
    let semitones = 12.0 * speed.log2();
    if semitones.abs() > MAX_DJ_SEMITONES {
        return None;
    }
    let b_keys = transpose(&track_b.keys, semitones.round() as i32);
    let category = if is_same_key(&track_a.keys, &b_keys) {
        Category::SameKeySameTempo
    } else {
        Category::SameTempoDifferentKey
    };
    let fade = DJ_FADE_SECONDS[rng.below(DJ_FADE_SECONDS.len())];
    Some(MixSpec {
        id: 0,
        a,
        b,
        a_id: track_a.id,
        b_id: track_b.id,
        transition: Transition::Dj(fade),
        category,
        a_seconds: A_MIN_SECONDS + rng.unit() * (A_MAX_SECONDS - A_MIN_SECONDS),
        b_speed: speed,
        a_keys: track_a.keys.clone(),
        b_keys,
        a_bpm: track_a.bpm,
        b_bpm: track_a.bpm,
    })
}

/// Decodes a track to mono at the analysis rate, gain-matched to `TARGET_RMS`.
pub fn load_track(track: &PoolTrack) -> Result<Vec<f32>, String> {
    let audio = decode::decode_mono(&track.audio_path)?;
    let mut samples = resample::to_analysis_rate(&audio)?;
    let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len().max(1) as f32).sqrt();
    if rms > 0.0 {
        let gain = TARGET_RMS / rms;
        samples.iter_mut().for_each(|sample| *sample *= gain);
    }
    Ok(samples)
}

pub fn render(spec: &MixSpec, tracks: &[PoolTrack]) -> Result<Mix, String> {
    let a = load_track(&tracks[spec.a])?;
    let mut b = load_track(&tracks[spec.b])?;
    if spec.b_speed != 1.0 {
        b = change_speed(&b, spec.b_speed);
    }
    let rate = ANALYSIS_SAMPLE_RATE as f32;
    let fade = spec.transition.fade_seconds();
    let mut fade_start = spec.a_seconds;
    if let (Transition::Dj(_), Some(bpm)) = (spec.transition, spec.a_bpm) {
        fade_start += beat_offset(BeatAlign {
            a: &a,
            b: &b,
            from_seconds: fade_start,
            bpm,
        });
    }
    let change_seconds = fade_start + fade / 2.0;
    let total_seconds = change_seconds + B_SECONDS;

    let start = (fade_start * rate) as usize;
    let fade_len = (fade * rate) as usize;
    let total = (total_seconds * rate) as usize;
    let needed_b = total - start;
    if a.len() < start + fade_len || b.len() < needed_b {
        return Err(format!(
            "tracks too short for mix {}: A {:.1}s, B {:.1}s",
            spec.id,
            a.len() as f32 / rate,
            b.len() as f32 / rate
        ));
    }

    let mut samples = vec![0.0; total];
    samples[..start].copy_from_slice(&a[..start]);
    for i in 0..needed_b {
        let position = if fade_len == 0 {
            1.0
        } else {
            (i as f32 / fade_len as f32).min(1.0)
        };
        let (gain_a, gain_b) = fade_gains(spec.transition, position);
        let from_a = if i < fade_len {
            a[start + i] * gain_a
        } else {
            0.0
        };
        samples[start + i] = from_a + b[i] * gain_b;
    }
    Ok(Mix {
        samples,
        change_seconds,
        fade_start,
    })
}

fn fade_gains(transition: Transition, position: f32) -> (f32, f32) {
    match transition {
        Transition::Cut => (0.0, 1.0),
        Transition::Linear(_) => (1.0 - position, position),
        Transition::EqualPower(_) | Transition::Dj(_) => {
            ((position * FRAC_PI_2).cos(), (position * FRAC_PI_2).sin())
        }
    }
}

/// Plays `samples` `speed` times faster (pitch moves with it), with Catmull-Rom interpolation.
fn change_speed(samples: &[f32], speed: f32) -> Vec<f32> {
    let len = (samples.len() as f32 / speed) as usize;
    let at = |index: isize| samples[index.clamp(0, samples.len() as isize - 1) as usize];
    (0..len)
        .map(|i| {
            let position = i as f32 * speed;
            let base = position.floor() as isize;
            let t = position - base as f32;
            let (p0, p1, p2, p3) = (at(base - 1), at(base), at(base + 1), at(base + 2));
            p1 + 0.5
                * t
                * (p2 - p0
                    + t * (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3 + t * (3.0 * (p1 - p2) + p3 - p0)))
        })
        .collect()
}

struct BeatAlign<'a> {
    a: &'a [f32],
    b: &'a [f32],
    from_seconds: f32,
    bpm: f32,
}

/// How much later than `from_seconds` B should start so its onsets line up with A's, under one beat.
fn beat_offset(align: BeatAlign) -> f32 {
    let rate = ANALYSIS_SAMPLE_RATE as f32;
    let beat_seconds = 60.0 / align.bpm;
    let a_start = (align.from_seconds * rate) as usize;
    let a_end = (a_start as f32 + (ALIGN_SECONDS + beat_seconds) * rate) as usize;
    let b_end = (ALIGN_SECONDS * rate) as usize;
    if align.a.len() < a_end || align.b.len() < b_end {
        return 0.0;
    }
    let flux_a = flux_of(&align.a[a_start..a_end]);
    let flux_b = flux_of(&align.b[..b_end]);
    let beat_frames = (beat_seconds * frame_rate()).round() as usize;
    let best = (0..beat_frames.max(1))
        .map(|shift| {
            let score: f32 = flux_b
                .iter()
                .zip(flux_a.iter().skip(shift))
                .map(|(b, a)| a * b)
                .sum();
            (shift, score)
        })
        .max_by(|x, y| x.1.total_cmp(&y.1))
        .map_or(0, |(shift, _)| shift);
    best as f32 / frame_rate()
}
