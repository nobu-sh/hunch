//! Spectral peak picking and harmonicity for one STFT frame.

/// A sinusoid's energy summed over Hann-windowed bins is its peak magnitude squared times this.
const HANN_ENBW_BINS: f32 = 1.5;
/// A peak must stand this far above the mean magnitude around it, which rejects noise.
const PEAK_TO_LOCAL_MEAN: f32 = 3.0;
/// Peaks quieter than this fraction of the frame's loudest bin (about -50 dB) are ignored.
const PEAK_FLOOR_RATIO: f32 = 0.003;
/// Half-width of the neighbourhood used for the local mean magnitude.
const LOCAL_MEAN_RADIUS_HZ: f32 = 40.0;
/// Frames whose band energy is below this (about -70 dBFS for a sine) are treated as silence.
const SILENCE_ENERGY: f32 = 1e-7;
/// Overtones that land on a different pitch class than their fundamental (fifths and thirds).
const NON_OCTAVE_HARMONICS: [f32; 4] = [3.0, 5.0, 6.0, 7.0];
/// How close, in cents, a peak must sit to a harmonic of a lower peak to count as its overtone.
const OVERTONE_TOLERANCE_CENTS: f32 = 25.0;
/// Keeps `ln` finite when interpolating around zero-magnitude bins.
const MIN_MAGNITUDE: f32 = 1e-12;

/// A spectral peak with interpolated frequency and magnitude (1.0 = full-scale sine).
#[derive(Debug, Clone, Copy)]
pub(super) struct Peak {
    pub frequency: f32,
    pub magnitude: f32,
    /// Sits on a fifth/third harmonic of a louder lower peak, so it is probably not a played note.
    pub is_overtone: bool,
}

/// What one non-silent frame contributed.
#[derive(Debug, Clone, Copy)]
pub(super) struct FrameSummary {
    /// 0..=1, share of the tonal band's energy that sits in spectral peaks.
    pub harmonicity: f32,
}

pub(super) struct PeakPickerSettings {
    pub sample_rate: u32,
    pub window_size: usize,
    pub bin_count: usize,
    /// Lowest frequency where peaks are collected.
    pub low_hz: f32,
    /// Lowest frequency counted for harmonicity, kept above kick drums.
    pub tonal_low_hz: f32,
    pub high_hz: f32,
}

/// Finds peaks in magnitude frames, reusing its buffers across frames.
pub(super) struct PeakPicker {
    bin_hz: f32,
    /// Scales raw FFT magnitudes so a full-scale sine peaks at 1.0.
    magnitude_scale: f32,
    low_bin: usize,
    tonal_low_bin: usize,
    high_bin: usize,
    local_mean_radius: usize,
    magnitudes: Vec<f32>,
    prefix_sums: Vec<f32>,
    peaks: Vec<Peak>,
}

impl PeakPicker {
    pub fn new(settings: PeakPickerSettings) -> Self {
        let bin_hz = settings.sample_rate as f32 / settings.window_size as f32;
        // Neighbouring bins are needed for interpolation, so stay off DC and Nyquist.
        let last_usable = settings.bin_count.saturating_sub(2);
        let to_bin = |hz: f32| ((hz / bin_hz).round() as usize).clamp(1, last_usable.max(1));
        Self {
            bin_hz,
            magnitude_scale: 4.0 / settings.window_size as f32,
            low_bin: to_bin(settings.low_hz),
            tonal_low_bin: to_bin(settings.tonal_low_hz),
            high_bin: to_bin(settings.high_hz),
            local_mean_radius: ((LOCAL_MEAN_RADIUS_HZ / bin_hz).round() as usize).max(2),
            magnitudes: vec![0.0; settings.bin_count],
            prefix_sums: vec![0.0; settings.bin_count + 1],
            peaks: Vec::new(),
        }
    }

    /// Peaks found by the last call to `analyze`.
    pub fn peaks(&self) -> &[Peak] {
        &self.peaks
    }

    /// Picks the peaks of `raw_magnitudes`. Returns `None` for silent frames, which have no peaks.
    pub fn analyze(&mut self, raw_magnitudes: &[f32]) -> Option<FrameSummary> {
        self.peaks.clear();
        if raw_magnitudes.len() != self.magnitudes.len() || self.low_bin >= self.high_bin {
            return None;
        }
        self.normalize(raw_magnitudes);

        let band = &self.magnitudes[self.low_bin..=self.high_bin];
        let band_energy: f32 = band.iter().map(|m| m * m).sum();
        if band_energy < SILENCE_ENERGY {
            return None;
        }
        let loudest = band.iter().copied().fold(0.0, f32::max);

        self.find_peaks(loudest * PEAK_FLOOR_RATIO);
        Some(FrameSummary {
            harmonicity: self.harmonicity(),
        })
    }

    fn normalize(&mut self, raw_magnitudes: &[f32]) {
        let mut running = 0.0;
        self.prefix_sums[0] = 0.0;
        for (bin, raw) in raw_magnitudes.iter().enumerate() {
            let magnitude = raw * self.magnitude_scale;
            self.magnitudes[bin] = magnitude;
            running += magnitude;
            self.prefix_sums[bin + 1] = running;
        }
    }

    fn local_mean(&self, bin: usize) -> f32 {
        let start = bin.saturating_sub(self.local_mean_radius);
        let end = (bin + self.local_mean_radius + 1).min(self.magnitudes.len());
        (self.prefix_sums[end] - self.prefix_sums[start]) / (end - start) as f32
    }

    fn find_peaks(&mut self, floor: f32) {
        for bin in self.low_bin..=self.high_bin {
            let magnitude = self.magnitudes[bin];
            let is_local_max =
                magnitude > self.magnitudes[bin - 1] && magnitude >= self.magnitudes[bin + 1];
            if !is_local_max || magnitude <= floor {
                continue;
            }
            if magnitude <= PEAK_TO_LOCAL_MEAN * self.local_mean(bin) {
                continue;
            }
            let peak = self.interpolate(bin);
            self.peaks.push(peak);
        }
        self.mark_overtones();
    }

    fn mark_overtones(&mut self) {
        for index in 0..self.peaks.len() {
            let peak = self.peaks[index];
            let is_overtone = self.peaks[..index].iter().any(|lower| {
                lower.magnitude >= peak.magnitude
                    && NON_OCTAVE_HARMONICS.iter().any(|harmonic| {
                        let cents = 1200.0 * (peak.frequency / (lower.frequency * harmonic)).log2();
                        cents.abs() < OVERTONE_TOLERANCE_CENTS
                    })
            });
            self.peaks[index].is_overtone = is_overtone;
        }
    }

    /// Parabolic interpolation on log magnitude around a local maximum.
    fn interpolate(&self, bin: usize) -> Peak {
        let before = self.magnitudes[bin - 1].max(MIN_MAGNITUDE).ln();
        let center = self.magnitudes[bin].max(MIN_MAGNITUDE).ln();
        let after = self.magnitudes[bin + 1].max(MIN_MAGNITUDE).ln();
        let curvature = before - 2.0 * center + after;
        let offset = if curvature < 0.0 {
            (0.5 * (before - after) / curvature).clamp(-0.5, 0.5)
        } else {
            0.0
        };
        let log_magnitude = center - 0.25 * (before - after) * offset;
        Peak {
            frequency: (bin as f32 + offset) * self.bin_hz,
            magnitude: log_magnitude.exp(),
            is_overtone: false,
        }
    }

    fn harmonicity(&self) -> f32 {
        let tonal_band = &self.magnitudes[self.tonal_low_bin..=self.high_bin];
        let tonal_energy: f32 = tonal_band.iter().map(|m| m * m).sum();
        if tonal_energy < SILENCE_ENERGY {
            return 0.0;
        }
        let tonal_low_hz = self.tonal_low_bin as f32 * self.bin_hz;
        let peak_energy: f32 = self
            .peaks
            .iter()
            .filter(|peak| peak.frequency >= tonal_low_hz)
            .map(|peak| HANN_ENBW_BINS * peak.magnitude * peak.magnitude)
            .sum();
        (peak_energy / tonal_energy).min(1.0)
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::PI;

    use super::*;
    use crate::dsp::Stft;

    const SAMPLE_RATE: u32 = 22_050;
    const WINDOW: usize = 8192;

    fn picker() -> PeakPicker {
        PeakPicker::new(PeakPickerSettings {
            sample_rate: SAMPLE_RATE,
            window_size: WINDOW,
            bin_count: WINDOW / 2 + 1,
            low_hz: 40.0,
            tonal_low_hz: 100.0,
            high_hz: 2500.0,
        })
    }

    fn first_frame(samples: &[f32]) -> Vec<f32> {
        let mut stft = Stft::new(WINDOW, WINDOW);
        let mut frames = Vec::new();
        stft.push(samples, |frame| frames.push(frame.to_vec()));
        frames.remove(0)
    }

    #[test]
    fn finds_sine_frequency_and_amplitude() {
        let samples: Vec<f32> = (0..WINDOW)
            .map(|i| 0.5 * (2.0 * PI * 443.3 * i as f32 / SAMPLE_RATE as f32).sin())
            .collect();
        let mut picker = picker();
        let summary = picker.analyze(&first_frame(&samples)).expect("not silent");

        let peaks = picker.peaks();
        assert_eq!(peaks.len(), 1);
        assert!((peaks[0].frequency - 443.3).abs() < 0.3, "{:?}", peaks[0]);
        assert!((peaks[0].magnitude - 0.5).abs() < 0.03, "{:?}", peaks[0]);
        assert!(summary.harmonicity > 0.9, "{summary:?}");
    }

    #[test]
    fn marks_fifth_harmonic_as_overtone() {
        let fundamental = 110.0;
        let samples: Vec<f32> = (0..WINDOW)
            .map(|i| {
                let t = i as f32 / SAMPLE_RATE as f32;
                (1..=5)
                    .map(|h| 0.2 / h as f32 * (2.0 * PI * fundamental * h as f32 * t).sin())
                    .sum()
            })
            .collect();
        let mut picker = picker();
        picker.analyze(&first_frame(&samples)).expect("not silent");

        let overtones: Vec<bool> = picker.peaks().iter().map(|peak| peak.is_overtone).collect();
        // Harmonics 1, 2 and 4 share the fundamental's pitch class; 3 and 5 do not.
        assert_eq!(overtones, [false, false, true, false, true]);
    }

    #[test]
    fn silence_has_no_summary() {
        let mut picker = picker();
        assert!(picker.analyze(&first_frame(&vec![0.0; WINDOW])).is_none());
        assert!(picker.peaks().is_empty());
    }
}
