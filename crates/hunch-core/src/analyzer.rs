use crate::Key;
use crate::rhythm::TempoDetector;
use crate::tonal::KeyDetector;

/// How much newly pushed audio, in seconds, is needed before the next estimate.
const ESTIMATE_INTERVAL_SECONDS: u32 = 1;

#[derive(Debug, Clone, PartialEq)]
pub struct Estimate {
    pub key: Option<Key>,
    pub key_confidence: f32,
    pub bpm: Option<f32>,
    pub bpm_confidence: f32,
    /// The other likely tempo octave, shown next to `bpm` when the octave call is unsure.
    pub bpm_alternate: Option<f32>,
    pub seconds_analyzed: f32,
}

/// Streaming analyzer: push mono audio in chunks, get estimates back as it refines.
pub struct Analyzer {
    sample_rate: u32,
    key: KeyDetector,
    tempo: TempoDetector,
    samples_per_estimate: u64,
    total_samples: u64,
    samples_since_estimate: u64,
}

impl Analyzer {
    pub fn new(sample_rate: u32) -> Self {
        // A zero rate would make the cadence math divide by zero; treat it as 1 Hz.
        let sample_rate = sample_rate.max(1);
        Self {
            sample_rate,
            key: KeyDetector::new(sample_rate),
            tempo: TempoDetector::new(sample_rate),
            samples_per_estimate: u64::from(sample_rate) * u64::from(ESTIMATE_INTERVAL_SECONDS),
            total_samples: 0,
            samples_since_estimate: 0,
        }
    }

    /// Mono f32 samples in [-1, 1]. Returns an estimate at most once per ~1 s of newly pushed audio.
    pub fn push(&mut self, samples: &[f32]) -> Option<Estimate> {
        self.key.push(samples);
        self.tempo.push(samples);
        let pushed = samples.len() as u64;
        self.total_samples += pushed;
        self.samples_since_estimate += pushed;

        if self.samples_since_estimate < self.samples_per_estimate {
            return None;
        }
        self.samples_since_estimate %= self.samples_per_estimate;
        Some(self.estimate())
    }

    pub fn reset(&mut self) {
        self.key.reset();
        self.tempo.reset();
        self.total_samples = 0;
        self.samples_since_estimate = 0;
    }

    fn estimate(&self) -> Estimate {
        let key = self.key.estimate();
        let tempo = self.tempo.estimate();
        Estimate {
            key: key.key,
            key_confidence: key.confidence,
            bpm: tempo.bpm,
            bpm_confidence: tempo.confidence,
            bpm_alternate: tempo.alternate,
            seconds_analyzed: self.total_samples as f32 / self.sample_rate as f32,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_RATE: u32 = 22_050;

    fn silence(seconds: f32) -> Vec<f32> {
        vec![0.0; (SAMPLE_RATE as f32 * seconds) as usize]
    }

    #[test]
    fn no_estimate_before_one_second() {
        let mut analyzer = Analyzer::new(SAMPLE_RATE);
        let chunk = silence(0.1);
        for _ in 0..9 {
            assert_eq!(analyzer.push(&chunk), None);
        }
    }

    #[test]
    fn one_estimate_after_one_second() {
        let mut analyzer = Analyzer::new(SAMPLE_RATE);
        let chunk = silence(0.1);
        let estimates: Vec<Estimate> = (0..15).filter_map(|_| analyzer.push(&chunk)).collect();

        assert_eq!(estimates.len(), 1);
        let estimate = &estimates[0];
        assert!((estimate.seconds_analyzed - 1.0).abs() < 1e-3);
        assert_eq!(estimate.key, None);
        assert_eq!(estimate.bpm, None);
        assert_eq!(estimate.key_confidence, 0.0);
        assert_eq!(estimate.bpm_confidence, 0.0);
    }

    #[test]
    fn large_push_yields_a_single_estimate() {
        let mut analyzer = Analyzer::new(SAMPLE_RATE);
        let estimate = analyzer.push(&silence(3.0)).expect("estimate after 3 s");
        assert!((estimate.seconds_analyzed - 3.0).abs() < 1e-3);
        assert_eq!(analyzer.push(&silence(0.1)), None);
    }

    #[test]
    fn reset_clears_progress() {
        let mut analyzer = Analyzer::new(SAMPLE_RATE);
        analyzer.push(&silence(0.9));
        analyzer.reset();
        assert_eq!(analyzer.push(&silence(0.5)), None);

        let estimate = analyzer.push(&silence(0.5)).expect("estimate after 1 s");
        assert!((estimate.seconds_analyzed - 1.0).abs() < 1e-3);
    }
}
