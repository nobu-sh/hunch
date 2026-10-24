//! Band-wise onset strength envelopes from log-compressed spectral flux.

use std::collections::VecDeque;

use crate::dsp::Stft;

/// Analysis window length, chosen per sample rate: ~46 ms (1024 samples at 22,050 Hz).
const WINDOW_SECONDS: f32 = 0.046;
/// Target envelope rate. The hop is rounded to whole samples, so the real rate is close to this.
const TARGET_FRAME_RATE: f32 = 100.0;
/// Scale for `ln(1 + γ·magnitude)`. Magnitudes are normalized so a full-scale sine is ~1.
const LOG_COMPRESSION: f32 = 100.0;
/// How much envelope history to keep. Older frames are dropped so memory stays bounded.
const HISTORY_SECONDS: f32 = 30.0;

/// The onset envelopes, by role in a beat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Band {
    /// Kick.
    Low,
    /// Snare, clap and the body of most instruments.
    Mid,
    /// Hi-hats and shakers.
    High,
    /// Mean of the other three, so kicks and snares weigh as much as the many hat bins.
    Combined,
}

/// Bands measured straight from the spectrum, with their range in Hz. Upper edges past Nyquist
/// are clamped to it.
const SPECTRAL_BANDS: [(Band, f32, f32); 3] = [
    (Band::Low, 30.0, 150.0),
    (Band::Mid, 150.0, 2_000.0),
    (Band::High, 5_000.0, 11_000.0),
];

impl Band {
    pub const ALL: [Band; 4] = [Band::Low, Band::Mid, Band::High, Band::Combined];

    pub fn index(self) -> usize {
        self as usize
    }
}

/// One envelope frame: onset strength per band, indexed by `Band::index`.
pub type OnsetFrame = [f32; Band::ALL.len()];

/// Streaming onset detector: pushes samples through an STFT and keeps a bounded envelope history.
pub struct OnsetEnvelopes {
    stft: Stft,
    frame_rate: f32,
    /// Normalizes Hann-windowed magnitudes so a full-scale sine peaks near 1.
    magnitude_scale: f32,
    /// Half-open bin range per entry of `SPECTRAL_BANDS`.
    band_bins: [(usize, usize); SPECTRAL_BANDS.len()],
    previous_log_spectrum: Vec<f32>,
    has_previous: bool,
    history: VecDeque<OnsetFrame>,
    history_capacity: usize,
}

impl OnsetEnvelopes {
    pub fn new(sample_rate: u32) -> Self {
        let sample_rate = sample_rate.max(1);
        let window_size = window_size_for(sample_rate);
        let hop_size =
            ((sample_rate as f32 / TARGET_FRAME_RATE).floor() as usize).clamp(1, window_size);
        let stft = Stft::new(window_size, hop_size);
        let frame_rate = sample_rate as f32 / hop_size as f32;
        let band_bins = SPECTRAL_BANDS
            .map(|(_, low_hz, high_hz)| band_bin_range(&stft, sample_rate, (low_hz, high_hz)));
        let history_capacity = (HISTORY_SECONDS * frame_rate).ceil() as usize;

        Self {
            previous_log_spectrum: vec![0.0; stft.bin_count()],
            magnitude_scale: 4.0 / window_size as f32,
            stft,
            frame_rate,
            band_bins,
            has_previous: false,
            history: VecDeque::with_capacity(history_capacity),
            history_capacity,
        }
    }

    /// Envelope frames per second.
    pub fn frame_rate(&self) -> f32 {
        self.frame_rate
    }

    /// The most frames `history` holds.
    pub fn history_capacity(&self) -> usize {
        self.history_capacity
    }

    /// Oldest frame first.
    pub fn history(&self) -> &VecDeque<OnsetFrame> {
        &self.history
    }

    pub fn push(&mut self, samples: &[f32]) {
        let Self {
            stft,
            magnitude_scale,
            band_bins,
            previous_log_spectrum,
            has_previous,
            history,
            history_capacity,
            ..
        } = self;

        stft.push(samples, |magnitudes| {
            let frame = spectral_flux(FluxInput {
                magnitudes,
                magnitude_scale: *magnitude_scale,
                band_bins,
                previous_log_spectrum,
            });
            // The first frame has nothing to diff against, so its flux would be the whole spectrum.
            if !*has_previous {
                *has_previous = true;
                return;
            }
            if history.len() == *history_capacity {
                history.pop_front();
            }
            history.push_back(frame);
        });
    }

    pub fn reset(&mut self) {
        self.stft.reset();
        self.previous_log_spectrum.fill(0.0);
        self.has_previous = false;
        self.history.clear();
    }
}

struct FluxInput<'a> {
    magnitudes: &'a [f32],
    magnitude_scale: f32,
    band_bins: &'a [(usize, usize); SPECTRAL_BANDS.len()],
    /// Updated in place to this frame's log spectrum.
    previous_log_spectrum: &'a mut [f32],
}

/// Half-wave rectified log-magnitude difference, averaged over each band's bins.
fn spectral_flux(input: FluxInput) -> OnsetFrame {
    let mut rises = [0.0; SPECTRAL_BANDS.len()];
    for (bin, (&magnitude, previous)) in input
        .magnitudes
        .iter()
        .zip(input.previous_log_spectrum.iter_mut())
        .enumerate()
    {
        let log_magnitude = (LOG_COMPRESSION * input.magnitude_scale * magnitude).ln_1p();
        let rise = (log_magnitude - *previous).max(0.0);
        *previous = log_magnitude;
        for (total, &(start, end)) in rises.iter_mut().zip(input.band_bins) {
            if (start..end).contains(&bin) {
                *total += rise;
            }
        }
    }

    let mut frame = [0.0; Band::ALL.len()];
    for (((band, _, _), rise), &(start, end)) in
        SPECTRAL_BANDS.iter().zip(rises).zip(input.band_bins)
    {
        let bin_count = end.saturating_sub(start).max(1);
        frame[band.index()] = rise / bin_count as f32;
    }
    frame[Band::Combined.index()] = spectral_mean(&frame);
    frame
}

fn spectral_mean(frame: &OnsetFrame) -> f32 {
    SPECTRAL_BANDS
        .iter()
        .map(|(band, _, _)| frame[band.index()])
        .sum::<f32>()
        / SPECTRAL_BANDS.len() as f32
}

/// Smallest power of two covering `WINDOW_SECONDS`.
fn window_size_for(sample_rate: u32) -> usize {
    let wanted = (sample_rate as f32 * WINDOW_SECONDS).ceil() as usize;
    wanted.max(2).next_power_of_two()
}

fn band_bin_range(stft: &Stft, sample_rate: u32, (low_hz, high_hz): (f32, f32)) -> (usize, usize) {
    let bin_width = stft.bin_frequency(1, sample_rate);
    let last_bin = stft.bin_count() - 1;
    let start = ((low_hz / bin_width).ceil() as usize).min(last_bin);
    let end = ((high_hz / bin_width).floor() as usize + 1).clamp(start + 1, last_bin + 1);
    (start, end)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_RATE: u32 = 22_050;

    #[test]
    fn uses_1024_window_and_100_fps_at_22050() {
        let envelopes = OnsetEnvelopes::new(SAMPLE_RATE);
        assert_eq!(window_size_for(SAMPLE_RATE), 1024);
        assert!((envelopes.frame_rate() - 100.23).abs() < 0.01);
    }

    #[test]
    fn history_is_bounded() {
        let mut envelopes = OnsetEnvelopes::new(SAMPLE_RATE);
        let minute = vec![0.0; SAMPLE_RATE as usize * 60];
        envelopes.push(&minute);
        assert_eq!(envelopes.history().len(), envelopes.history_capacity);
    }

    #[test]
    fn click_rises_in_every_band() {
        let mut envelopes = OnsetEnvelopes::new(SAMPLE_RATE);
        let mut signal = vec![0.0; SAMPLE_RATE as usize];
        signal[SAMPLE_RATE as usize / 2] = 1.0;
        envelopes.push(&signal);

        for band in Band::ALL {
            let peak = envelopes
                .history()
                .iter()
                .map(|frame| frame[band.index()])
                .fold(0.0, f32::max);
            assert!(peak > 0.1, "{band:?} peak {peak}");
        }
    }
}
