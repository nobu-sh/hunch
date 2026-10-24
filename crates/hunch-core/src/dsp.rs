//! Shared DSP building blocks.

use std::f32::consts::PI;
use std::sync::Arc;

use realfft::num_complex::Complex32;
use realfft::{RealFftPlanner, RealToComplex};

/// Streaming short-time Fourier transform: push samples, get Hann-windowed magnitude frames.
pub struct Stft {
    window_size: usize,
    hop_size: usize,
    window: Vec<f32>,
    fft: Arc<dyn RealToComplex<f32>>,
    /// Samples not yet consumed by a full frame. Always shorter than `window_size + hop_size`.
    pending: Vec<f32>,
    frame: Vec<f32>,
    spectrum: Vec<Complex32>,
    scratch: Vec<Complex32>,
}

impl Stft {
    /// `window_size` must be even and `hop_size` must be in `1..=window_size`.
    pub fn new(window_size: usize, hop_size: usize) -> Self {
        assert!(
            window_size >= 2 && window_size % 2 == 0,
            "window size must be even"
        );
        assert!(
            (1..=window_size).contains(&hop_size),
            "hop size must be in 1..=window size"
        );

        let fft = RealFftPlanner::<f32>::new().plan_fft_forward(window_size);
        let window = (0..window_size)
            .map(|i| 0.5 - 0.5 * (2.0 * PI * i as f32 / window_size as f32).cos())
            .collect();
        Self {
            window_size,
            hop_size,
            window,
            spectrum: fft.make_output_vec(),
            scratch: fft.make_scratch_vec(),
            frame: vec![0.0; window_size],
            fft,
            pending: Vec::with_capacity(window_size + hop_size),
        }
    }

    pub fn window_size(&self) -> usize {
        self.window_size
    }

    pub fn hop_size(&self) -> usize {
        self.hop_size
    }

    /// Number of magnitude bins per frame: `window_size / 2 + 1`, from DC to Nyquist.
    pub fn bin_count(&self) -> usize {
        self.window_size / 2 + 1
    }

    /// Center frequency of `bin` in Hz.
    pub fn bin_frequency(&self, bin: usize, sample_rate: u32) -> f32 {
        bin as f32 * sample_rate as f32 / self.window_size as f32
    }

    /// Feeds samples and calls `on_frame` with the magnitude spectrum of every completed frame.
    pub fn push(&mut self, samples: &[f32], mut on_frame: impl FnMut(&[f32])) {
        let mut magnitudes = vec![0.0; self.bin_count()];
        let mut input = samples;
        while !input.is_empty() {
            let wanted = self.window_size - self.pending.len().min(self.window_size);
            let take = wanted.min(input.len());
            self.pending.extend_from_slice(&input[..take]);
            input = &input[take..];

            if self.pending.len() < self.window_size {
                break;
            }
            self.transform(&mut magnitudes);
            on_frame(&magnitudes);
            self.pending.drain(..self.hop_size);
        }
    }

    pub fn reset(&mut self) {
        self.pending.clear();
    }

    fn transform(&mut self, magnitudes: &mut [f32]) {
        for ((out, sample), weight) in self.frame.iter_mut().zip(&self.pending).zip(&self.window) {
            *out = sample * weight;
        }
        // Lengths all come from the same plan, so the FFT cannot fail.
        self.fft
            .process_with_scratch(&mut self.frame, &mut self.spectrum, &mut self.scratch)
            .expect("buffers sized by the FFT plan");
        for (magnitude, bin) in magnitudes.iter_mut().zip(&self.spectrum) {
            *magnitude = bin.norm();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_RATE: u32 = 22_050;

    fn sine(frequency: f32, samples: usize) -> Vec<f32> {
        (0..samples)
            .map(|i| (2.0 * PI * frequency * i as f32 / SAMPLE_RATE as f32).sin())
            .collect()
    }

    fn collect_frames(stft: &mut Stft, samples: &[f32]) -> Vec<Vec<f32>> {
        let mut frames = Vec::new();
        stft.push(samples, |frame| frames.push(frame.to_vec()));
        frames
    }

    #[test]
    fn frame_count_follows_hop() {
        let mut stft = Stft::new(1024, 256);
        let frames = collect_frames(&mut stft, &vec![0.0; 1024 + 256 * 3]);
        assert_eq!(frames.len(), 4);
        assert!(frames.iter().all(|frame| frame.len() == 513));
    }

    #[test]
    fn chunked_push_matches_single_push() {
        let signal = sine(440.0, 10_000);
        let mut whole = Stft::new(2048, 512);
        let expected = collect_frames(&mut whole, &signal);

        let mut chunked = Stft::new(2048, 512);
        let mut actual = Vec::new();
        for chunk in signal.chunks(777) {
            actual.extend(collect_frames(&mut chunked, chunk));
        }
        assert_eq!(actual, expected);
    }

    #[test]
    fn sine_peaks_at_its_bin() {
        let mut stft = Stft::new(4096, 4096);
        let frames = collect_frames(&mut stft, &sine(440.0, 4096));
        let peak = frames[0]
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map(|(bin, _)| bin)
            .expect("non-empty frame");
        assert!((stft.bin_frequency(peak, SAMPLE_RATE) - 440.0).abs() < 6.0);
    }
}
