use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Resampler};

use crate::decode::MonoAudio;

/// The rate the extension's AudioContext runs at, so the rate every eval run analyzes at.
pub const ANALYSIS_SAMPLE_RATE: u32 = 22_050;
/// Rubato's FFT resampler processes the clip in chunks of this many input frames.
const RESAMPLER_CHUNK_FRAMES: usize = 1024;
const MONO: usize = 1;

/// Converts `audio` to `ANALYSIS_SAMPLE_RATE` with rubato's band-limited FFT resampler.
pub fn to_analysis_rate(audio: &MonoAudio) -> Result<Vec<f32>, String> {
    let input_rate = audio.sample_rate;
    if input_rate == ANALYSIS_SAMPLE_RATE || audio.samples.is_empty() {
        return Ok(audio.samples.clone());
    }

    let mut resampler = Fft::<f32>::new(
        input_rate as usize,
        ANALYSIS_SAMPLE_RATE as usize,
        RESAMPLER_CHUNK_FRAMES,
        MONO,
        FixedSync::Input,
    )
    .map_err(|error| format!("cannot resample from {input_rate} Hz: {error}"))?;

    let frames = audio.samples.len();
    let input = InterleavedSlice::new(&audio.samples, MONO, frames)
        .map_err(|error| format!("bad resampler input: {error}"))?;
    let output = resampler
        .process_all(&input, frames, None)
        .map_err(|error| format!("resampling from {input_rate} Hz failed: {error}"))?;
    Ok(output.take_data())
}

/// Repeats a seamless loop end to end until it is at least `min_len` samples long.
pub fn repeat_to_length(samples: Vec<f32>, min_len: usize) -> Vec<f32> {
    if samples.is_empty() || samples.len() >= min_len {
        return samples;
    }
    samples
        .iter()
        .copied()
        .cycle()
        .take(min_len.next_multiple_of(samples.len()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const INPUT_RATE: u32 = 44_100;
    const SINE_HZ: f32 = 1_000.0;

    fn sine(sample_rate: u32, seconds: f32) -> MonoAudio {
        let len = (sample_rate as f32 * seconds) as usize;
        let samples = (0..len)
            .map(|n| (n as f32 / sample_rate as f32 * SINE_HZ * std::f32::consts::TAU).sin())
            .collect();
        MonoAudio {
            samples,
            sample_rate,
        }
    }

    #[test]
    fn resamples_to_analysis_rate_preserving_length_and_level() {
        let resampled = to_analysis_rate(&sine(INPUT_RATE, 2.0)).expect("resample");

        assert_eq!(resampled.len(), ANALYSIS_SAMPLE_RATE as usize * 2);
        let middle = &resampled[1_000..resampled.len() - 1_000];
        let peak = middle.iter().fold(0.0f32, |max, s| max.max(s.abs()));
        assert!((peak - 1.0).abs() < 0.02, "peak was {peak}");
    }

    #[test]
    fn resampled_sine_keeps_its_frequency() {
        let resampled = to_analysis_rate(&sine(INPUT_RATE, 1.0)).expect("resample");
        let rising_zero_crossings = resampled
            .windows(2)
            .filter(|pair| pair[0] < 0.0 && pair[1] >= 0.0)
            .count();
        assert!(
            rising_zero_crossings.abs_diff(SINE_HZ as usize) <= 2,
            "{rising_zero_crossings} crossings"
        );
    }

    #[test]
    fn leaves_audio_at_analysis_rate_untouched() {
        let audio = sine(ANALYSIS_SAMPLE_RATE, 0.5);
        assert_eq!(to_analysis_rate(&audio).expect("resample"), audio.samples);
    }

    #[test]
    fn repeats_whole_loops_until_long_enough() {
        assert_eq!(
            repeat_to_length(vec![1.0, 2.0], 5),
            vec![1.0, 2.0, 1.0, 2.0, 1.0, 2.0]
        );
        assert_eq!(repeat_to_length(vec![1.0, 2.0], 1), vec![1.0, 2.0]);
        assert!(repeat_to_length(Vec::new(), 10).is_empty());
    }
}
