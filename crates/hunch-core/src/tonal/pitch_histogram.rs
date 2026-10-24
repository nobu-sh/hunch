//! Octave-folded pitch histogram at 1-cent resolution, so tuning can be applied after the fact.

use super::PITCH_CLASSES;

const CENTS_PER_SEMITONE: usize = 100;
const CENTS_PER_OCTAVE: usize = CENTS_PER_SEMITONE * PITCH_CLASSES;
/// C4 at A4 = 440 Hz; bin 0 of the histogram is C.
const C_REFERENCE_HZ: f32 = 261.625_58;
/// Half-width of the triangular kernel that smooths the tuning histogram.
const TUNING_SMOOTHING_CENTS: usize = 8;

/// Weighted pitch evidence folded into one octave, 1 cent per bin, bin 0 = C at A440.
pub(super) struct PitchHistogram {
    bins: Vec<f32>,
}

impl PitchHistogram {
    pub fn new() -> Self {
        Self {
            bins: vec![0.0; CENTS_PER_OCTAVE],
        }
    }

    pub fn add(&mut self, frequency: f32, weight: f32) {
        if frequency <= 0.0 || !weight.is_finite() {
            return;
        }
        let cents = (1200.0 * (frequency / C_REFERENCE_HZ).log2()).rem_euclid(1200.0);
        // `rem_euclid` can round up to exactly 1200.0, so clamp to the last bin.
        let bin = (cents as usize).min(CENTS_PER_OCTAVE - 1);
        self.bins[bin] += weight;
    }

    pub fn reset(&mut self) {
        self.bins.fill(0.0);
    }

    /// Sums the evidence into 12 pitch classes, with semitone boundaries shifted by `tuning_cents`.
    pub fn fold(&self, tuning_cents: f32) -> [f32; PITCH_CLASSES] {
        let mut chroma = [0.0; PITCH_CLASSES];
        for (bin, weight) in self.bins.iter().enumerate() {
            let semitones = (bin as f32 + 0.5 - tuning_cents) / CENTS_PER_SEMITONE as f32;
            let pitch_class = (semitones.round() as i32).rem_euclid(PITCH_CLASSES as i32) as usize;
            chroma[pitch_class] += weight;
        }
        chroma
    }
}

/// Global deviation from A440 in cents, in `-50.0..50.0`, from the combined histograms.
pub(super) fn estimate_tuning(histograms: &[&PitchHistogram]) -> f32 {
    let mut deviations = [0.0f32; CENTS_PER_SEMITONE];
    for histogram in histograms {
        for (bin, weight) in histogram.bins.iter().enumerate() {
            deviations[bin % CENTS_PER_SEMITONE] += weight;
        }
    }
    let smoothed = smooth_circular(&deviations);
    let Some(best) = argmax(&smoothed) else {
        return 0.0;
    };
    if smoothed[best] <= 0.0 {
        return 0.0;
    }

    let before = smoothed[(best + CENTS_PER_SEMITONE - 1) % CENTS_PER_SEMITONE];
    let after = smoothed[(best + 1) % CENTS_PER_SEMITONE];
    let deviation = best as f32 + 0.5 + parabolic_offset(before, smoothed[best], after);
    let half = CENTS_PER_SEMITONE as f32 / 2.0;
    (deviation + half).rem_euclid(CENTS_PER_SEMITONE as f32) - half
}

fn smooth_circular(values: &[f32; CENTS_PER_SEMITONE]) -> [f32; CENTS_PER_SEMITONE] {
    let mut smoothed = [0.0; CENTS_PER_SEMITONE];
    let radius = TUNING_SMOOTHING_CENTS as isize;
    for (center, out) in smoothed.iter_mut().enumerate() {
        for offset in -radius..=radius {
            let index = (center as isize + offset).rem_euclid(CENTS_PER_SEMITONE as isize);
            let weight = 1.0 - offset.unsigned_abs() as f32 / (radius + 1) as f32;
            *out += weight * values[index as usize];
        }
    }
    smoothed
}

fn argmax(values: &[f32]) -> Option<usize> {
    values
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map(|(index, _)| index)
}

fn parabolic_offset(before: f32, center: f32, after: f32) -> f32 {
    let curvature = before - 2.0 * center + after;
    if curvature >= 0.0 {
        return 0.0;
    }
    (0.5 * (before - after) / curvature).clamp(-0.5, 0.5)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hz(midi: f32) -> f32 {
        440.0 * 2f32.powf((midi - 69.0) / 12.0)
    }

    #[test]
    fn folds_notes_to_their_pitch_class() {
        let mut histogram = PitchHistogram::new();
        histogram.add(hz(60.0), 1.0); // C4
        histogram.add(hz(45.0), 2.0); // A2
        histogram.add(hz(78.0), 3.0); // F#5
        let chroma = histogram.fold(0.0);
        assert_eq!(chroma[0], 1.0);
        assert_eq!(chroma[9], 2.0);
        assert_eq!(chroma[6], 3.0);
        assert_eq!(chroma.iter().sum::<f32>(), 6.0);
    }

    #[test]
    fn estimates_sharp_and_flat_tuning() {
        for cents in [-40.0, -12.0, 0.0, 23.0, 35.0] {
            let mut histogram = PitchHistogram::new();
            for midi in [48.0, 55.0, 64.0, 67.0, 72.0] {
                histogram.add(hz(midi + cents / 100.0), 1.0);
            }
            let tuning = estimate_tuning(&[&histogram]);
            assert!((tuning - cents).abs() < 2.0, "{cents} -> {tuning}");
        }
    }

    #[test]
    fn detuned_notes_fold_to_nearest_pitch_class_after_tuning() {
        let mut histogram = PitchHistogram::new();
        histogram.add(hz(60.45), 1.0);
        assert_eq!(histogram.fold(45.0)[0], 1.0);
        assert_eq!(histogram.fold(-45.0)[1], 1.0);
    }

    #[test]
    fn empty_histogram_has_no_tuning() {
        assert_eq!(estimate_tuning(&[&PitchHistogram::new()]), 0.0);
    }
}
