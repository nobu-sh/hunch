//! Key detection.
//!
//! Spectral peaks from a long-window STFT are folded into an octave-wide pitch histogram at 1-cent
//! resolution, weighted by how harmonic each frame is so drum hits barely count. At estimate time
//! the histogram's tuning offset is found, it is folded into a 12-bin chroma (bass and treble
//! weighted separately), and the chroma is correlated with 24 major/minor key profiles.

mod peaks;
mod pitch_histogram;
mod profiles;

pub use profiles::KeyProfile;

use crate::Key;
use crate::dsp::Stft;
use peaks::{PeakPicker, PeakPickerSettings};
use pitch_histogram::{PitchHistogram, estimate_tuning};
use profiles::rank_keys;

const PITCH_CLASSES: usize = 12;

/// ~0.37 s: 8192 samples at 22,050 Hz, enough resolution to separate bass notes.
const WINDOW_SECONDS: f32 = 8192.0 / 22_050.0;
const HOP_DIVISOR: usize = 4;
const MIN_WINDOW_SIZE: usize = 256;
const MAX_WINDOW_SIZE: usize = 1 << 16;
const LOW_HZ: f32 = 40.0;
const HIGH_HZ: f32 = 2500.0;
/// Harmonicity ignores energy below this so kick drums don't read as pitched.
const TONAL_LOW_HZ: f32 = 100.0;
/// Peaks below this go into the bass histogram.
const BASS_CUTOFF_HZ: f32 = 130.0;
/// Log compression of peak magnitudes (1.0 = full-scale sine) before accumulating.
const MAGNITUDE_COMPRESSION: f32 = 100.0;
/// Overtones still count a little: a louder lower peak may itself be an overtone or another note.
const OVERTONE_WEIGHT: f32 = 0.3;
/// Best-vs-rival correlation margin that counts as full confidence.
const FULL_CONFIDENCE_MARGIN: f32 = 0.15;
/// Seconds of fully harmonic audio after which evidence stops limiting confidence (to ~63%).
const EVIDENCE_TIME_CONSTANT_SECONDS: f32 = 2.0;

#[derive(Debug, Clone, PartialEq)]
pub struct KeyEstimate {
    /// `None` when the audio has no clear tonal center (drums, noise, speech).
    pub key: Option<Key>,
    /// 0..=1, how likely `key` is the right scale. 0 when `key` is `None`.
    pub confidence: f32,
    /// 0..=1, how pitched/harmonic the audio is. Used to decide whether to report a key at all.
    pub tonalness: f32,
}

/// Tunable parameters for [`KeyDetector`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KeyDetectorConfig {
    pub profile: KeyProfile,
    /// Below this tonalness no key is reported.
    pub tonalness_threshold: f32,
    /// Weight of the bass chroma relative to the treble chroma; each is normalized first.
    pub bass_weight: f32,
}

impl Default for KeyDetectorConfig {
    fn default() -> Self {
        Self {
            profile: KeyProfile::default(),
            tonalness_threshold: 0.15,
            bass_weight: 0.25,
        }
    }
}

/// Streaming key detector. Accumulates evidence over everything pushed since the last reset.
pub struct KeyDetector {
    config: KeyDetectorConfig,
    stft: Stft,
    evidence: Evidence,
}

impl KeyDetector {
    pub fn new(sample_rate: u32) -> Self {
        Self::with_config(sample_rate, KeyDetectorConfig::default())
    }

    pub fn with_config(sample_rate: u32, config: KeyDetectorConfig) -> Self {
        let sample_rate = sample_rate.max(1);
        let window_size = window_size_for(sample_rate);
        let hop_size = window_size / HOP_DIVISOR;
        let stft = Stft::new(window_size, hop_size);
        let picker = PeakPicker::new(PeakPickerSettings {
            sample_rate,
            window_size,
            bin_count: stft.bin_count(),
            low_hz: LOW_HZ,
            tonal_low_hz: TONAL_LOW_HZ,
            high_hz: HIGH_HZ.min(0.45 * sample_rate as f32),
        });
        Self {
            config,
            stft,
            evidence: Evidence::new(picker, hop_size as f32 / sample_rate as f32),
        }
    }

    pub fn config(&self) -> &KeyDetectorConfig {
        &self.config
    }

    /// Mono f32 samples in [-1, 1] at the rate given to `new`.
    pub fn push(&mut self, samples: &[f32]) {
        let evidence = &mut self.evidence;
        self.stft
            .push(samples, |magnitudes| evidence.add_frame(magnitudes));
    }

    pub fn estimate(&self) -> KeyEstimate {
        let tonalness = self.evidence.tonalness();
        let ranking = rank_keys(&self.chroma(), self.config.profile);
        let Some(ranking) = ranking.filter(|_| tonalness >= self.config.tonalness_threshold) else {
            return KeyEstimate {
                key: None,
                confidence: 0.0,
                tonalness,
            };
        };

        let margin = (ranking.best_score - ranking.rival_score).max(0.0);
        let separation = (margin / FULL_CONFIDENCE_MARGIN).min(1.0);
        KeyEstimate {
            key: Some(ranking.best),
            confidence: separation * self.evidence.sufficiency(),
            tonalness,
        }
    }

    /// Deviation of the audio from A = 440 Hz in cents, in `-50.0..50.0`. 0 before any pitch is heard.
    pub fn tuning_cents(&self) -> f32 {
        estimate_tuning(&[&self.evidence.bass, &self.evidence.treble])
    }

    /// Tuning-corrected pitch-class profile, index 0 = C, scaled so the largest bin is 1.
    /// All zeros before any pitch is heard.
    pub fn chroma(&self) -> [f32; PITCH_CLASSES] {
        let tuning = self.tuning_cents();
        let treble = normalized_sum(self.evidence.treble.fold(tuning));
        let bass = normalized_sum(self.evidence.bass.fold(tuning));
        let mut chroma: [f32; PITCH_CLASSES] =
            std::array::from_fn(|pitch| treble[pitch] + self.config.bass_weight * bass[pitch]);

        let largest = chroma.iter().copied().fold(0.0, f32::max);
        if largest > 0.0 {
            chroma.iter_mut().for_each(|value| *value /= largest);
        }
        chroma
    }

    pub fn reset(&mut self) {
        self.stft.reset();
        self.evidence.reset();
    }
}

/// Everything accumulated from frames since the last reset.
struct Evidence {
    picker: PeakPicker,
    hop_seconds: f32,
    bass: PitchHistogram,
    treble: PitchHistogram,
    /// Frames that weren't silent.
    active_frames: u32,
    harmonicity_sum: f32,
    /// Seconds of audio, each frame counted by its harmonicity.
    tonal_seconds: f32,
}

impl Evidence {
    fn new(picker: PeakPicker, hop_seconds: f32) -> Self {
        Self {
            picker,
            hop_seconds,
            bass: PitchHistogram::new(),
            treble: PitchHistogram::new(),
            active_frames: 0,
            harmonicity_sum: 0.0,
            tonal_seconds: 0.0,
        }
    }

    fn add_frame(&mut self, magnitudes: &[f32]) {
        let Some(summary) = self.picker.analyze(magnitudes) else {
            return;
        };
        let frame_weight = summary.harmonicity;
        self.active_frames += 1;
        self.harmonicity_sum += frame_weight;
        self.tonal_seconds += frame_weight * self.hop_seconds;

        for peak in self.picker.peaks() {
            let overtone_weight = if peak.is_overtone {
                OVERTONE_WEIGHT
            } else {
                1.0
            };
            let weight =
                frame_weight * overtone_weight * (MAGNITUDE_COMPRESSION * peak.magnitude).ln_1p();
            let histogram = if peak.frequency < BASS_CUTOFF_HZ {
                &mut self.bass
            } else {
                &mut self.treble
            };
            histogram.add(peak.frequency, weight);
        }
    }

    /// Mean harmonicity of the non-silent frames.
    fn tonalness(&self) -> f32 {
        if self.active_frames == 0 {
            return 0.0;
        }
        self.harmonicity_sum / self.active_frames as f32
    }

    /// 0..=1, grows with the amount of harmonic audio heard.
    fn sufficiency(&self) -> f32 {
        1.0 - (-self.tonal_seconds / EVIDENCE_TIME_CONSTANT_SECONDS).exp()
    }

    fn reset(&mut self) {
        self.bass.reset();
        self.treble.reset();
        self.active_frames = 0;
        self.harmonicity_sum = 0.0;
        self.tonal_seconds = 0.0;
    }
}

/// Power-of-two window closest (in log terms) to `WINDOW_SECONDS` at `sample_rate`.
fn window_size_for(sample_rate: u32) -> usize {
    let target = (sample_rate as f32 * WINDOW_SECONDS).max(1.0);
    let exponent = target.log2().round() as u32;
    2usize.pow(exponent).clamp(MIN_WINDOW_SIZE, MAX_WINDOW_SIZE)
}

fn normalized_sum(values: [f32; PITCH_CLASSES]) -> [f32; PITCH_CLASSES] {
    let total: f32 = values.iter().sum();
    if total <= 0.0 {
        return [0.0; PITCH_CLASSES];
    }
    values.map(|value| value / total)
}

#[cfg(test)]
mod tests;
