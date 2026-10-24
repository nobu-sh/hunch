//! How often the whole onset pattern repeats: a seamless loop's length, or a phrase in longer
//! audio. A loop holds a whole number of beats, usually a power of two, so its length tells which
//! tempi are consistent with it.

use std::sync::Arc;

use realfft::num_complex::Complex32;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};

use super::periodicity::parabolic_offset;

/// Lags whose overlap is shorter than this fraction of the envelope are not searched. Lower than
/// the beat autocorrelation's, so a loop heard just twice still shows its length.
const MIN_OVERLAP_FRACTION: f32 = 0.4;
/// A segment variance below this is treated as flat, where correlation is undefined.
const MIN_VARIANCE: f64 = 1e-12;
/// Lags whose correlation is within this of the best are as good; the shortest of them wins, so a
/// loop heard three times reports its length rather than twice it.
const SAME_STRENGTH_MARGIN: f32 = 0.02;
/// Correlation range over which a repetition goes from meaningless to a trusted loop length.
/// Below the minimum, most repetitions are bar-level similarity in longer audio, not loops.
const MIN_LOOP_STRENGTH: f32 = 0.5;
const FULL_LOOP_STRENGTH: f32 = 0.95;

/// The repetition period of an envelope.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Repetition {
    /// In frames, refined to a fraction.
    pub lag: f32,
    /// Pearson correlation of the envelope with itself shifted by `lag`: ~1 for a seamless loop.
    pub strength: f32,
}

impl Repetition {
    /// 0..=1 trust in the repetition as a loop length: 0 at or below `MIN_LOOP_STRENGTH`, 1 from
    /// `FULL_LOOP_STRENGTH`, where a seamless loop sits.
    pub fn weight(&self) -> f32 {
        let span = FULL_LOOP_STRENGTH - MIN_LOOP_STRENGTH;
        ((self.strength - MIN_LOOP_STRENGTH) / span).clamp(0.0, 1.0)
    }
}

/// Correlates an envelope with itself at every lag via one FFT round trip.
pub struct RepetitionFinder {
    forward: Arc<dyn RealToComplex<f32>>,
    inverse: Arc<dyn ComplexToReal<f32>>,
    /// FFT length: at least twice the longest envelope, so the correlation doesn't wrap around.
    size: usize,
}

impl RepetitionFinder {
    /// Handles envelopes up to `max_frames` long.
    pub fn new(max_frames: usize) -> Self {
        let size = (2 * max_frames.max(1)).next_power_of_two();
        let mut planner = RealFftPlanner::<f32>::new();
        Self {
            forward: planner.plan_fft_forward(size),
            inverse: planner.plan_fft_inverse(size),
            size,
        }
    }

    /// The best-correlated lag of at least `shortest_lag` frames. `None` for an envelope that's too
    /// short or flat to correlate, or longer than `new` allowed.
    pub fn find(&self, envelope: &[f32], shortest_lag: usize) -> Option<Repetition> {
        let frame_count = envelope.len();
        let longest_lag = (frame_count as f32 * (1.0 - MIN_OVERLAP_FRACTION)) as usize;
        if 2 * frame_count > self.size || shortest_lag.max(1) + 1 >= longest_lag {
            return None;
        }

        let products = self.lagged_products(envelope)?;
        let sums = PrefixSums::new(envelope);
        let strengths: Vec<f32> = (0..=longest_lag + 1)
            .map(|lag| sums.correlation(lag, products[lag]))
            .collect();

        let search = shortest_lag.max(1)..=longest_lag;
        let best = search
            .clone()
            .map(|lag| strengths[lag])
            .fold(f32::NEG_INFINITY, f32::max);
        if best <= 0.0 {
            return None;
        }
        let lag = search.into_iter().find(|&lag| {
            let is_peak =
                strengths[lag] >= strengths[lag - 1] && strengths[lag] >= strengths[lag + 1];
            is_peak && strengths[lag] >= best - SAME_STRENGTH_MARGIN
        })?;
        let offset = parabolic_offset([strengths[lag - 1], strengths[lag], strengths[lag + 1]]);
        Some(Repetition {
            lag: lag as f32 + offset,
            strength: strengths[lag],
        })
    }

    /// `Σᵢ x[i]·x[i + lag]` for every lag, by the Wiener–Khinchin theorem.
    fn lagged_products(&self, envelope: &[f32]) -> Option<Vec<f32>> {
        let mut signal = vec![0.0; self.size];
        signal[..envelope.len()].copy_from_slice(envelope);
        let mut spectrum = self.forward.make_output_vec();
        self.forward.process(&mut signal, &mut spectrum).ok()?;
        for bin in &mut spectrum {
            *bin = Complex32::new(bin.norm_sqr(), 0.0);
        }
        self.inverse.process(&mut spectrum, &mut signal).ok()?;
        let scale = self.size as f32;
        Some(signal.iter().map(|value| value / scale).collect())
    }
}

/// Running sums of the envelope and its square, for the mean and variance of any segment.
struct PrefixSums {
    values: Vec<f64>,
    squares: Vec<f64>,
}

impl PrefixSums {
    fn new(envelope: &[f32]) -> Self {
        let mut values = vec![0.0; envelope.len() + 1];
        let mut squares = vec![0.0; envelope.len() + 1];
        for (index, &value) in envelope.iter().enumerate() {
            let value = f64::from(value);
            values[index + 1] = values[index] + value;
            squares[index + 1] = squares[index] + value * value;
        }
        Self { values, squares }
    }

    /// Mean and variance of frames `start..end`.
    fn moments(&self, start: usize, end: usize) -> (f64, f64) {
        let count = (end - start) as f64;
        let mean = (self.values[end] - self.values[start]) / count;
        let mean_square = (self.squares[end] - self.squares[start]) / count;
        (mean, mean_square - mean * mean)
    }

    /// Pearson correlation of the first and last `len − lag` frames, given their summed product.
    fn correlation(&self, lag: usize, product_sum: f32) -> f32 {
        let frame_count = self.values.len() - 1;
        if lag >= frame_count {
            return 0.0;
        }
        let overlap = frame_count - lag;
        let (early_mean, early_variance) = self.moments(0, overlap);
        let (late_mean, late_variance) = self.moments(lag, frame_count);
        if early_variance <= MIN_VARIANCE || late_variance <= MIN_VARIANCE {
            return 0.0;
        }
        let covariance = f64::from(product_sum) / overlap as f64 - early_mean * late_mean;
        (covariance / (early_variance * late_variance).sqrt()) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A non-repeating pattern of `length` frames, tiled to `frames`.
    fn tiled(length: usize, frames: usize) -> Vec<f32> {
        let pattern: Vec<f32> = (0..length)
            .map(|frame| ((frame * 7_919) % 13) as f32 / 13.0)
            .collect();
        pattern.iter().copied().cycle().take(frames).collect()
    }

    #[test]
    fn finds_the_loop_length() {
        let finder = RepetitionFinder::new(3_000);
        let repetition = finder
            .find(&tiled(437, 1_500), 50)
            .expect("a repeating envelope");
        assert!((repetition.lag - 437.0).abs() < 0.5, "{repetition:?}");
        assert!(repetition.strength > 0.99, "{repetition:?}");
    }

    #[test]
    fn prefers_the_loop_over_its_multiples() {
        let finder = RepetitionFinder::new(3_000);
        let repetition = finder
            .find(&tiled(300, 2_000), 50)
            .expect("a repeating envelope");
        assert!((repetition.lag - 300.0).abs() < 0.5, "{repetition:?}");
    }

    #[test]
    fn flat_or_oversized_envelopes_have_no_repetition() {
        let finder = RepetitionFinder::new(1_000);
        assert_eq!(finder.find(&[0.5; 800], 50), None);
        assert_eq!(finder.find(&tiled(100, 1_500), 50), None);
    }
}
