//! Autocorrelation of onset envelopes and the search for the dominant beat period.

/// Lags whose overlap with the envelope is shorter than this fraction of it are too noisy to use.
const MIN_OVERLAP_FRACTION: f32 = 0.5;
/// Envelope variance below this, absolute or relative to the squared mean, is treated as flat.
const MIN_VARIANCE: f32 = 1e-12;
const MIN_RELATIVE_VARIANCE: f32 = 1e-6;
/// Multiples of a lag summed by the comb, so the true pulse collects evidence from its whole bar.
const COMB_HARMONICS: usize = 4;
/// How far, as a fraction of the lag, `peak_near` looks for a peak. Absorbs small tempo drift.
const PEAK_TOLERANCE_FRACTION: f32 = 0.02;
/// `peak_near` always looks at least this many frames either side.
const MIN_PEAK_TOLERANCE_FRAMES: f32 = 1.0;
/// Center and width (in octaves) of the mild tempo weighting on the dominant-period search.
/// It only picks which member of the tempo family seeds the candidates, not the final octave.
const SEARCH_PRIOR_CENTER_BPM: f32 = 120.0;
const SEARCH_PRIOR_WIDTH_OCTAVES: f32 = 1.5;

/// Normalized autocorrelation of one envelope: `values[0] == 1`, and each lag is
/// the mean-removed correlation over the overlapping frames, divided by the variance.
pub struct Autocorrelation {
    values: Vec<f32>,
}

impl Autocorrelation {
    /// `None` when the envelope is constant (silence), since correlation is undefined then.
    pub fn new(envelope: &[f32], max_lag: usize) -> Option<Self> {
        let frame_count = envelope.len();
        if frame_count < 2 {
            return None;
        }
        let mean = envelope.iter().sum::<f32>() / frame_count as f32;
        let centered: Vec<f32> = envelope.iter().map(|value| value - mean).collect();
        let variance = centered.iter().map(|value| value * value).sum::<f32>() / frame_count as f32;
        if variance <= MIN_VARIANCE.max(MIN_RELATIVE_VARIANCE * mean * mean) {
            return None;
        }

        let usable_lags = usable_lag_count(frame_count).min(max_lag + 1);
        let values = (0..usable_lags)
            .map(|lag| {
                let overlap = frame_count - lag;
                let sum: f32 = centered[..overlap]
                    .iter()
                    .zip(&centered[lag..])
                    .map(|(a, b)| a * b)
                    .sum();
                sum / overlap as f32 / variance
            })
            .collect();
        Some(Self { values })
    }

    /// Linearly interpolated correlation at a fractional lag; 0 outside the usable range.
    pub fn at(&self, lag: f32) -> f32 {
        if lag.is_nan() || lag < 0.0 {
            return 0.0;
        }
        let below = lag.floor() as usize;
        let Some(&low) = self.values.get(below) else {
            return 0.0;
        };
        let Some(&high) = self.values.get(below + 1) else {
            return if lag == below as f32 { low } else { 0.0 };
        };
        let fraction = lag - below as f32;
        low + (high - low) * fraction
    }

    /// Whether `lag` has enough overlap to be measured.
    pub fn covers(&self, lag: f32) -> bool {
        lag >= 0.0 && lag <= (self.values.len() - 1) as f32
    }

    /// Strongest correlation within a small tolerance of `lag`.
    pub fn peak_near(&self, lag: f32) -> f32 {
        let tolerance = (lag * PEAK_TOLERANCE_FRACTION).max(MIN_PEAK_TOLERANCE_FRAMES);
        let start = (lag - tolerance).max(0.0).ceil() as usize;
        let end = (lag + tolerance).floor() as usize;
        let interior = (start..=end).map(|whole| self.at(whole as f32));
        interior
            .chain([self.at(lag)])
            .fold(f32::NEG_INFINITY, f32::max)
    }

    /// Mean correlation over the first `COMB_HARMONICS` multiples of `lag` that are measurable.
    pub fn comb(&self, lag: f32) -> f32 {
        let (sum, count) = (1..=COMB_HARMONICS)
            .map(|harmonic| lag * harmonic as f32)
            .take_while(|&multiple| self.covers(multiple))
            .fold((0.0, 0), |(sum, count), multiple| {
                (sum + self.at(multiple), count + 1)
            });
        if count == 0 {
            return 0.0;
        }
        sum / count as f32
    }
}

/// A beat period found in an envelope's autocorrelation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Period {
    /// In frames, refined to a fraction.
    pub lag: f32,
    /// Comb strength at `lag`, before the search prior. Roughly 0 for noise, up to 1 for a strict loop.
    pub strength: f32,
}

/// Lag search bounds, in frames.
pub struct LagRange {
    pub shortest: usize,
    pub longest: usize,
}

/// Local maxima of the prior-weighted comb over `range`, strongest first, each refined by
/// parabolic interpolation.
pub fn strongest_periods(
    autocorrelation: &Autocorrelation,
    range: LagRange,
    frame_rate: f32,
) -> Vec<Period> {
    let weighted = |lag: f32| autocorrelation.comb(lag) * search_prior(lag, frame_rate);
    let scores: Vec<(usize, f32)> = (range.shortest..=range.longest)
        .filter(|&lag| lag > 0 && autocorrelation.covers(lag as f32))
        .map(|lag| (lag, weighted(lag as f32)))
        .collect();

    let mut peaks: Vec<(f32, f32)> = scores
        .windows(3)
        .filter(|window| window[1].1 > window[0].1 && window[1].1 >= window[2].1)
        .map(|window| {
            let lag = window[1].0 as f32;
            let offset = parabolic_offset([window[0].1, window[1].1, window[2].1]);
            (lag + offset, window[1].1)
        })
        .collect();
    peaks.sort_by(|a, b| b.1.total_cmp(&a.1));
    peaks
        .into_iter()
        .map(|(lag, _)| Period {
            lag,
            strength: autocorrelation.comb(lag),
        })
        .collect()
}

/// Converts between a beat period in frames and a tempo.
pub fn lag_to_bpm(lag: f32, frame_rate: f32) -> f32 {
    60.0 * frame_rate / lag
}

pub fn bpm_to_lag(bpm: f32, frame_rate: f32) -> f32 {
    60.0 * frame_rate / bpm
}

/// The longest lag the envelope can measure with enough overlap.
fn usable_lag_count(frame_count: usize) -> usize {
    ((frame_count as f32 * (1.0 - MIN_OVERLAP_FRACTION)) as usize).max(1)
}

/// Log-Gaussian weight around `SEARCH_PRIOR_CENTER_BPM`, 1 at the center.
fn search_prior(lag: f32, frame_rate: f32) -> f32 {
    let octaves = (lag_to_bpm(lag, frame_rate) / SEARCH_PRIOR_CENTER_BPM).log2();
    let spread = octaves / SEARCH_PRIOR_WIDTH_OCTAVES;
    (-0.5 * spread * spread).exp()
}

/// Vertex offset of the parabola through three equally spaced points, clamped to ±0.5.
fn parabolic_offset([left, center, right]: [f32; 3]) -> f32 {
    let curvature = left - 2.0 * center + right;
    if curvature >= 0.0 {
        return 0.0;
    }
    (0.5 * (left - right) / curvature).clamp(-0.5, 0.5)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pulse_train(period: usize, frames: usize) -> Vec<f32> {
        (0..frames)
            .map(|frame| if frame % period == 0 { 1.0 } else { 0.0 })
            .collect()
    }

    #[test]
    fn constant_envelope_has_no_autocorrelation() {
        assert!(Autocorrelation::new(&[0.0; 500], 100).is_none());
        assert!(Autocorrelation::new(&[0.3; 500], 100).is_none());
    }

    #[test]
    fn pulse_train_peaks_at_its_period() {
        let acf = Autocorrelation::new(&pulse_train(50, 2_000), 400).expect("periodic envelope");
        assert!((acf.at(0.0) - 1.0).abs() < 1e-4);
        assert!(acf.at(50.0) > 0.9);
        assert!(acf.at(25.0) < 0.1);
    }

    #[test]
    fn strongest_period_is_the_pulse() {
        let acf = Autocorrelation::new(&pulse_train(50, 2_000), 400).expect("periodic envelope");
        let range = LagRange {
            shortest: 30,
            longest: 100,
        };
        let period = strongest_periods(&acf, range, 100.0)[0];
        assert!((period.lag - 50.0).abs() < 0.5, "lag {}", period.lag);
        assert!(period.strength > 0.9);
    }

    #[test]
    fn parabola_vertex() {
        assert!((parabolic_offset([0.0, 1.0, 0.0])).abs() < 1e-6);
        assert!(parabolic_offset([0.0, 1.0, 0.5]) > 0.0);
        assert_eq!(parabolic_offset([1.0, 0.0, 1.0]), 0.0);
    }
}
