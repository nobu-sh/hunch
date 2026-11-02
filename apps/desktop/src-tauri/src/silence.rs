//! Notices a source that delivers only digital silence, which for an app or output device most
//! likely means protected (DRM) audio or nothing playing.

use std::time::Duration;

/// How long capture runs on nothing but silence before the source is reported silent.
pub const SILENCE_TIMEOUT: Duration = Duration::from_secs(15);
/// Peaks at or below this (about -120 dBFS) count as digital silence.
pub const DIGITAL_SILENCE_PEAK: f32 = 1e-6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SilenceChange {
    /// The source has been silent since capture started, for `SILENCE_TIMEOUT`.
    BecameSilent,
    /// Audio arrived after the source was reported silent.
    HeardAudio,
}

/// One session's silence decision. Reports silence at most once.
pub struct SilenceDetector {
    is_watching: bool,
    has_heard_audio: bool,
    has_reported_silence: bool,
}

pub struct Observation<'a> {
    /// Time since capture started.
    pub elapsed: Duration,
    /// What arrived since the last observation; empty when nothing did.
    pub samples: &'a [f32],
}

impl SilenceDetector {
    /// `is_watching` is false for sources that may rightly be quiet, such as a mic.
    pub fn new(is_watching: bool) -> Self {
        Self {
            is_watching,
            has_heard_audio: false,
            has_reported_silence: false,
        }
    }

    pub fn observe(&mut self, observation: Observation<'_>) -> Option<SilenceChange> {
        if !self.is_watching || self.has_heard_audio {
            return None;
        }
        if has_audio(observation.samples) {
            self.has_heard_audio = true;
            return self
                .has_reported_silence
                .then_some(SilenceChange::HeardAudio);
        }
        let is_overdue = observation.elapsed >= SILENCE_TIMEOUT;
        if is_overdue && !self.has_reported_silence {
            self.has_reported_silence = true;
            return Some(SilenceChange::BecameSilent);
        }
        None
    }
}

fn has_audio(samples: &[f32]) -> bool {
    samples
        .iter()
        .any(|sample| sample.abs() > DIGITAL_SILENCE_PEAK)
}

#[cfg(test)]
mod tests;
