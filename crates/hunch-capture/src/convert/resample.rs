use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Resampler};

use crate::CaptureError;

/// Rubato's FFT resampler converts this many input frames per step.
const RESAMPLER_CHUNK_FRAMES: usize = 1024;
const MONO: usize = 1;

/// Streaming band-limited resampler for mono audio. Input is held until a whole step is buffered,
/// so output lags input by up to one step plus the filter delay.
pub struct StreamResampler {
    /// `None` when the rates already match.
    resampler: Option<Fft<f32>>,
    pending: Vec<f32>,
    scratch: Vec<f32>,
}

#[derive(Debug, Clone, Copy)]
pub struct Rates {
    pub input: u32,
    pub output: u32,
}

impl StreamResampler {
    pub fn new(rates: Rates) -> Result<Self, CaptureError> {
        if rates.input == rates.output {
            return Ok(Self {
                resampler: None,
                pending: Vec::new(),
                scratch: Vec::new(),
            });
        }
        let resampler = Fft::<f32>::new(
            rates.input as usize,
            rates.output as usize,
            RESAMPLER_CHUNK_FRAMES,
            MONO,
            FixedSync::Input,
        )
        .map_err(|error| {
            CaptureError::Backend(format!(
                "cannot resample {} Hz to {} Hz: {error}",
                rates.input, rates.output
            ))
        })?;
        let scratch = vec![0.0; resampler.output_frames_max()];
        Ok(Self {
            resampler: Some(resampler),
            pending: Vec::new(),
            scratch,
        })
    }

    /// Resamples whatever whole steps `samples` completes. Returns an empty vec while buffering.
    pub fn push(&mut self, samples: &[f32]) -> Result<Vec<f32>, CaptureError> {
        let Some(resampler) = self.resampler.as_mut() else {
            return Ok(samples.to_vec());
        };
        self.pending.extend_from_slice(samples);

        let mut output = Vec::new();
        let mut consumed = 0;
        loop {
            let needed = resampler.input_frames_next();
            let available = self.pending.len() - consumed;
            if available < needed {
                break;
            }
            let input_samples = &self.pending[consumed..consumed + needed];
            let input = InterleavedSlice::new(input_samples, MONO, needed)
                .map_err(|error| CaptureError::Backend(format!("bad resampler input: {error}")))?;
            let scratch_frames = self.scratch.len();
            let mut scratch = InterleavedSlice::new_mut(&mut self.scratch, MONO, scratch_frames)
                .map_err(|error| CaptureError::Backend(format!("bad resampler output: {error}")))?;
            let (read, written) = resampler
                .process_into_buffer(&input, &mut scratch, None)
                .map_err(|error| CaptureError::Backend(format!("resampling failed: {error}")))?;
            output.extend_from_slice(&self.scratch[..written]);
            consumed += read;
        }
        self.pending.drain(..consumed);
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::TAU;

    use super::*;

    const INPUT_RATE: u32 = 48_000;
    const OUTPUT_RATE: u32 = 22_050;
    const SINE_HZ: f32 = 1_000.0;
    /// Pushes arrive in WASAPI-sized packets of 10 ms.
    const PACKET_FRAMES: usize = 480;

    fn sine(sample_rate: u32, seconds: f32) -> Vec<f32> {
        let len = (sample_rate as f32 * seconds) as usize;
        (0..len)
            .map(|n| (n as f32 / sample_rate as f32 * SINE_HZ * TAU).sin())
            .collect()
    }

    fn resample_in_packets(samples: &[f32]) -> Vec<f32> {
        let mut resampler = StreamResampler::new(Rates {
            input: INPUT_RATE,
            output: OUTPUT_RATE,
        })
        .expect("resampler");
        let mut output = Vec::new();
        for packet in samples.chunks(PACKET_FRAMES) {
            output.extend(resampler.push(packet).expect("resample"));
        }
        output
    }

    fn upward_zero_crossings(samples: &[f32]) -> usize {
        samples
            .windows(2)
            .filter(|pair| pair[0] < 0.0 && pair[1] >= 0.0)
            .count()
    }

    #[test]
    fn output_length_follows_the_rate_ratio() {
        let input = sine(INPUT_RATE, 2.0);

        let output = resample_in_packets(&input);

        let ratio = f64::from(OUTPUT_RATE) / f64::from(INPUT_RATE);
        let expected = input.len() as f64 * ratio;
        // Up to one resampler step of input can still be buffered.
        let max_buffered = RESAMPLER_CHUNK_FRAMES as f64 * ratio;
        let shortfall = expected - output.len() as f64;
        assert!(
            (0.0..=max_buffered).contains(&shortfall),
            "expected about {expected} samples, got {}",
            output.len()
        );
    }

    #[test]
    fn a_sine_keeps_its_frequency() {
        let output = resample_in_packets(&sine(INPUT_RATE, 2.0));

        // Skip the filter's start-up delay, then count cycles over one second of output.
        let one_second = OUTPUT_RATE as usize;
        let settled = &output[2_000..2_000 + one_second];
        let cycles = upward_zero_crossings(settled) as f32;
        assert!((cycles - SINE_HZ).abs() <= 1.0, "counted {cycles} cycles");
    }

    #[test]
    fn matching_rates_pass_through() {
        let mut resampler = StreamResampler::new(Rates {
            input: OUTPUT_RATE,
            output: OUTPUT_RATE,
        })
        .expect("resampler");

        assert_eq!(resampler.push(&[0.1, 0.2]).expect("push"), [0.1, 0.2]);
    }
}
