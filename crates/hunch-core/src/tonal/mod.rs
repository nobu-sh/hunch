//! Key detection.
//!
//! Spectral peaks from a long-window STFT are folded into an octave-wide pitch histogram at 1-cent
//! resolution, weighted by how harmonic each frame is so drum hits barely count. At estimate time
//! the histogram's tuning offset is found, it is folded into a 12-bin chroma (bass and treble
//! weighted separately), and the chroma is correlated with 24 major/minor key profiles. The bass
//! notes also vote for the tonic. A logistic presence gate on tonalness and how key-like the chroma
//! is decides whether to report a key at all (`hunch-eval key-sweep --fit-gate`). Confidence is a
//! logistic calibration of how clearly the best key wins, how tonal the audio is and how key-like
//! the chroma is, fitted by `hunch-eval key-sweep --calibrate`.

mod peaks;
mod pitch_histogram;
mod profiles;

pub use profiles::KeyProfile;

use crate::Key;
use crate::dsp::Stft;
use peaks::{Peak, PeakPicker, PeakPickerSettings};
use pitch_histogram::{PitchHistogram, estimate_tuning};
use profiles::{KeyScoring, rank_keys};

const PITCH_CLASSES: usize = 12;

/// ~0.37 s: 8192 samples at 22,050 Hz, enough resolution to separate bass notes.
const WINDOW_SECONDS: f32 = 8192.0 / 22_050.0;
const HOP_DIVISOR: usize = 4;
const MIN_WINDOW_SIZE: usize = 256;
const MAX_WINDOW_SIZE: usize = 1 << 16;
/// Harmonicity ignores energy below this so kick drums don't read as pitched.
const TONAL_LOW_HZ: f32 = 100.0;
/// Peaks and harmonicity are measured up to here; `high_hz` only limits what reaches the chroma.
const ANALYSIS_HIGH_HZ: f32 = 2500.0;
/// Peaks are only collected below this fraction of the sample rate, clear of the anti-alias filter.
const MAX_HIGH_HZ_RATIO: f32 = 0.45;
/// Best-vs-rival correlation margin that counts as full separation.
const FULL_SEPARATION_MARGIN: f32 = 0.15;
/// `confidence = evidence · σ(BIAS + SEPARATION_WEIGHT · separation + TONALNESS_WEIGHT ·
/// tonalness + CLARITY_WEIGHT · clarity)`, fitted on FSL10K (min 15 s) and Beatport (30 s windows) train with
/// the default config by `hunch-eval key-sweep --calibrate`. Atonal FSL10K false keys count as
/// wrong and carry 20% of FSL10K's weight.
const CONFIDENCE_BIAS: f32 = -4.772;
const CONFIDENCE_SEPARATION_WEIGHT: f32 = 1.073;
const CONFIDENCE_TONALNESS_WEIGHT: f32 = 0.813;
const CONFIDENCE_CLARITY_WEIGHT: f32 = 5.035;
/// `presence = σ(BIAS + TONALNESS_WEIGHT · tonalness + CLARITY_WEIGHT · clarity)`, fitted on train
/// keyed sounds (FSL10K loops, Beatport 30 s windows) against FSL10K atonal loops, each group
/// weighted equally, by `hunch-eval key-sweep --fit-gate`.
const GATE_BIAS: f32 = -3.368;
const GATE_TONALNESS_WEIGHT: f32 = 4.446;
const GATE_CLARITY_WEIGHT: f32 = 4.198;
/// Seconds of fully harmonic audio after which evidence stops limiting confidence (to ~63%).
const EVIDENCE_TIME_CONSTANT_SECONDS: f32 = 2.0;
/// Score lead a new key needs over the held one before it replaces it. Picked on validation
/// timelines (`hunch-eval timeline`): halves how often the shown key changes at equal accuracy.
const HOLD_MARGIN: f32 = 0.06;

#[derive(Debug, Clone, PartialEq)]
pub struct KeyEstimate {
    /// `None` when the audio has no clear tonal center (drums, noise, speech).
    pub key: Option<Key>,
    /// 0..=1, how likely `key` is the right scale (exact or relative). 0 when `key` is `None`.
    pub confidence: f32,
    /// 0..=1, how pitched/harmonic the audio is.
    pub tonalness: f32,
    /// 0..=1, how likely the audio has a key at all, from tonalness and clarity. 0 when the chroma
    /// is flat.
    pub presence: f32,
    /// 0..=1, how clearly the best key beats keys with other notes. 0 when `key` is `None`.
    pub separation: f32,
    /// Correlation of the chroma with the best key's profile, plus its tonic cue: how key-like the
    /// pitch content is. 0 when the chroma is flat.
    pub clarity: f32,
    /// 0..=1, grows with the amount of harmonic audio heard.
    pub evidence: f32,
}

/// Tunable parameters for [`KeyDetector`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KeyDetectorConfig {
    pub profile: KeyProfile,
    /// Below this presence no key is reported.
    pub presence_threshold: f32,
    /// Below this tonalness no key is reported, whatever the presence.
    pub tonalness_threshold: f32,
    /// Weight of the bass chroma relative to the treble chroma; each is normalized first.
    pub bass_weight: f32,
    /// Score added to a key in proportion to how much the bass sits on its tonic.
    pub tonic_weight: f32,
    /// Lowest peak frequency collected.
    pub low_hz: f32,
    /// Highest peak frequency that counts toward the chroma.
    pub high_hz: f32,
    /// Peaks below this go into the bass histogram.
    pub bass_cutoff_hz: f32,
    /// Weight of peaks marked as overtones. Not 0: a louder lower peak may itself be an overtone
    /// or another note.
    pub overtone_weight: f32,
    /// Frames count by harmonicity raised to this power; higher suppresses drum-heavy frames more.
    pub harmonicity_exponent: f32,
    /// Peaks count as `ln(1 + magnitude_compression · magnitude)`, 1.0 magnitude = full-scale sine.
    pub magnitude_compression: f32,
    /// Experimental, off by default: evidence fades with this exponential time constant instead of
    /// accumulating for the whole window, so a new key can take over. `None` keeps everything.
    pub forget_seconds: Option<f32>,
    /// Experimental, for `hunch-eval mix-bench`: also tally evidence per block, read and cleared by
    /// `KeyDetector::take_block`. Doesn't change estimates.
    pub track_blocks: bool,
}

impl Default for KeyDetectorConfig {
    fn default() -> Self {
        Self {
            profile: KeyProfile::default(),
            presence_threshold: 0.65,
            tonalness_threshold: 0.0,
            bass_weight: 0.0,
            tonic_weight: 0.06,
            low_hz: 60.0,
            high_hz: 2500.0,
            bass_cutoff_hz: 130.0,
            overtone_weight: 0.3,
            harmonicity_exponent: 1.0,
            magnitude_compression: 100.0,
            forget_seconds: None,
            track_blocks: false,
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
            low_hz: config.low_hz,
            tonal_low_hz: TONAL_LOW_HZ,
            high_hz: ANALYSIS_HIGH_HZ.min(MAX_HIGH_HZ_RATIO * sample_rate as f32),
        });
        Self {
            config,
            stft,
            evidence: Evidence::new(EvidenceSettings {
                picker,
                hop_seconds: hop_size as f32 / sample_rate as f32,
                forget_seconds: config.forget_seconds,
                track_blocks: config.track_blocks,
            }),
        }
    }

    pub fn config(&self) -> &KeyDetectorConfig {
        &self.config
    }

    /// Mono f32 samples in [-1, 1] at the rate given to `new`.
    pub fn push(&mut self, samples: &[f32]) {
        let evidence = &mut self.evidence;
        let config = &self.config;
        self.stft
            .push(samples, |magnitudes| evidence.add_frame(magnitudes, config));
    }

    pub fn estimate(&self) -> KeyEstimate {
        self.estimate_holding(None)
    }

    /// Like `estimate`, but keeps `held` (the key shown so far) unless another key beats it by
    /// more than `HOLD_MARGIN`. The presence gate still judges the best key.
    pub fn estimate_holding(&self, held: Option<Key>) -> KeyEstimate {
        let tonalness = self.evidence.total.tonalness();
        let tuning = self.tuning_cents();
        let scoring = KeyScoring {
            profile: self.config.profile,
            tonic_cue: scaled_to_max(self.evidence.total.bass.fold(tuning)),
            tonic_weight: self.config.tonic_weight,
        };
        let ranking = rank_keys(&self.chroma_at(tuning), scoring);
        let evidence = self.evidence.total.sufficiency();
        let no_key = |presence: f32, clarity: f32| KeyEstimate {
            key: None,
            confidence: 0.0,
            tonalness,
            presence,
            separation: 0.0,
            clarity,
            evidence,
        };
        let Some(ranking) = ranking else {
            return no_key(0.0, 0.0);
        };
        let clarity = ranking.best_score;
        let presence =
            sigmoid(GATE_BIAS + GATE_TONALNESS_WEIGHT * tonalness + GATE_CLARITY_WEIGHT * clarity);
        let is_keyless = presence < self.config.presence_threshold
            || tonalness < self.config.tonalness_threshold;
        if is_keyless {
            return no_key(presence, clarity);
        }
        let ranking = match held {
            Some(held) => ranking.holding(held, HOLD_MARGIN),
            None => ranking,
        };

        let margin = (ranking.best_score - ranking.rival_score).max(0.0);
        let separation = (margin / FULL_SEPARATION_MARGIN).min(1.0);
        let logit = CONFIDENCE_BIAS
            + CONFIDENCE_SEPARATION_WEIGHT * separation
            + CONFIDENCE_TONALNESS_WEIGHT * tonalness
            + CONFIDENCE_CLARITY_WEIGHT * clarity;
        KeyEstimate {
            key: Some(ranking.best),
            confidence: evidence * sigmoid(logit),
            tonalness,
            presence,
            separation,
            clarity,
            evidence,
        }
    }

    /// Deviation of the audio from A = 440 Hz in cents, in `-50.0..50.0`. 0 before any pitch is heard.
    pub fn tuning_cents(&self) -> f32 {
        estimate_tuning(&[&self.evidence.total.bass, &self.evidence.total.treble])
    }

    /// Tuning-corrected pitch-class profile, index 0 = C, scaled so the largest bin is 1.
    /// All zeros before any pitch is heard.
    pub fn chroma(&self) -> [f32; PITCH_CLASSES] {
        self.chroma_at(self.tuning_cents())
    }

    /// Tuning-corrected pitch classes of the bass notes (the tonic cue), scaled like `chroma`.
    pub fn bass_chroma(&self) -> [f32; PITCH_CLASSES] {
        scaled_to_max(self.evidence.total.bass.fold(self.tuning_cents()))
    }

    fn chroma_at(&self, tuning: f32) -> [f32; PITCH_CLASSES] {
        let treble = normalized_sum(self.evidence.total.treble.fold(tuning));
        let bass = normalized_sum(self.evidence.total.bass.fold(tuning));
        scaled_to_max(std::array::from_fn(|pitch| {
            treble[pitch] + self.config.bass_weight * bass[pitch]
        }))
    }

    /// Experimental, for `hunch-eval mix-bench`: the evidence since the previous call, folded at the current tuning, then
    /// clears it. `None` unless `track_blocks` is set.
    pub fn take_block(&mut self) -> Option<ChromaBlock> {
        let tuning = self.tuning_cents();
        let block = self.evidence.block.as_mut()?;
        let taken = ChromaBlock {
            treble: block.treble.fold(tuning),
            bass: block.bass.fold(tuning),
            active_frames: block.active_frames,
            harmonicity_sum: block.harmonicity_sum,
            tonal_seconds: block.tonal_seconds,
        };
        block.reset();
        Some(taken)
    }

    pub fn reset(&mut self) {
        self.stft.reset();
        self.evidence.reset();
    }
}

/// Unnormalized evidence over a stretch of frames, so blocks can be summed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChromaBlock {
    pub treble: [f32; PITCH_CLASSES],
    pub bass: [f32; PITCH_CLASSES],
    /// Frames that weren't silent.
    pub active_frames: f32,
    pub harmonicity_sum: f32,
    /// Seconds of audio, each frame counted by its harmonicity.
    pub tonal_seconds: f32,
}

struct EvidenceSettings {
    picker: PeakPicker,
    hop_seconds: f32,
    forget_seconds: Option<f32>,
    track_blocks: bool,
}

/// Peak picking plus everything accumulated from its frames.
struct Evidence {
    picker: PeakPicker,
    hop_seconds: f32,
    /// Everything since the last reset, faded per frame when `forget_seconds` is set.
    total: Tally,
    /// Multiplier applied to `total` before each frame is added.
    fade_per_frame: Option<f32>,
    /// Frames since the last `take_block`, when blocks are tracked.
    block: Option<Tally>,
}

impl Evidence {
    fn new(settings: EvidenceSettings) -> Self {
        let fade_per_frame = settings
            .forget_seconds
            .map(|seconds| (-settings.hop_seconds / seconds.max(f32::MIN_POSITIVE)).exp());
        Self {
            picker: settings.picker,
            hop_seconds: settings.hop_seconds,
            total: Tally::new(),
            fade_per_frame,
            block: settings.track_blocks.then(Tally::new),
        }
    }

    fn add_frame(&mut self, magnitudes: &[f32], config: &KeyDetectorConfig) {
        let Some(summary) = self.picker.analyze(magnitudes) else {
            return;
        };
        let frame = Frame {
            peaks: self.picker.peaks(),
            harmonicity: summary.harmonicity,
            hop_seconds: self.hop_seconds,
            config,
        };
        if let Some(fade) = self.fade_per_frame {
            self.total.fade(fade);
        }
        self.total.add(&frame);
        if let Some(block) = self.block.as_mut() {
            block.add(&frame);
        }
    }

    fn reset(&mut self) {
        self.total.reset();
        if let Some(block) = self.block.as_mut() {
            block.reset();
        }
    }
}

/// One analyzed, non-silent frame.
struct Frame<'a> {
    /// In ascending frequency order.
    peaks: &'a [Peak],
    harmonicity: f32,
    hop_seconds: f32,
    config: &'a KeyDetectorConfig,
}

/// Pitch evidence and frame statistics accumulated from frames.
struct Tally {
    bass: PitchHistogram,
    treble: PitchHistogram,
    /// Frames that weren't silent.
    active_frames: f32,
    harmonicity_sum: f32,
    /// Seconds of audio, each frame counted by its harmonicity.
    tonal_seconds: f32,
}

impl Tally {
    fn new() -> Self {
        Self {
            bass: PitchHistogram::new(),
            treble: PitchHistogram::new(),
            active_frames: 0.0,
            harmonicity_sum: 0.0,
            tonal_seconds: 0.0,
        }
    }

    fn add(&mut self, frame: &Frame) {
        let config = frame.config;
        self.active_frames += 1.0;
        self.harmonicity_sum += frame.harmonicity;
        self.tonal_seconds += frame.harmonicity * frame.hop_seconds;

        let frame_weight = frame.harmonicity.powf(config.harmonicity_exponent);
        for peak in frame.peaks {
            if peak.frequency > config.high_hz {
                break;
            }
            let overtone_weight = if peak.is_overtone {
                config.overtone_weight
            } else {
                1.0
            };
            let weight = frame_weight
                * overtone_weight
                * (config.magnitude_compression * peak.magnitude).ln_1p();
            let histogram = if peak.frequency < config.bass_cutoff_hz {
                &mut self.bass
            } else {
                &mut self.treble
            };
            histogram.add(peak.frequency, weight);
        }
    }

    fn fade(&mut self, factor: f32) {
        self.bass.scale(factor);
        self.treble.scale(factor);
        self.active_frames *= factor;
        self.harmonicity_sum *= factor;
        self.tonal_seconds *= factor;
    }

    /// Mean harmonicity of the non-silent frames.
    fn tonalness(&self) -> f32 {
        if self.active_frames <= 0.0 {
            return 0.0;
        }
        self.harmonicity_sum / self.active_frames
    }

    /// 0..=1, grows with the amount of harmonic audio heard.
    fn sufficiency(&self) -> f32 {
        1.0 - (-self.tonal_seconds / EVIDENCE_TIME_CONSTANT_SECONDS).exp()
    }

    fn reset(&mut self) {
        self.bass.reset();
        self.treble.reset();
        self.active_frames = 0.0;
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

fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

/// `values` divided by their largest entry; unchanged when none is positive.
fn scaled_to_max(mut values: [f32; PITCH_CLASSES]) -> [f32; PITCH_CLASSES] {
    let largest = values.iter().copied().fold(0.0, f32::max);
    if largest > 0.0 {
        values.iter_mut().for_each(|value| *value /= largest);
    }
    values
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
