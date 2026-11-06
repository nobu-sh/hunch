//! A cheap spectral front end for the novelty and beat-grid approaches: an all-band onset
//! envelope and a log band-energy (timbre) descriptor. In `hunch-core` this would reuse the tempo
//! detector's onset STFT, which has the same size.

use hunch_core::dsp::Stft;

use crate::resample::ANALYSIS_SAMPLE_RATE;

const WINDOW_SIZE: usize = 1024;
const HOP_SIZE: usize = 256;
const LOG_COMPRESSION: f32 = 100.0;
const LOW_HZ: f32 = 60.0;
const HIGH_HZ: f32 = 10_000.0;
pub const TIMBRE_BANDS: usize = 12;

pub fn frame_rate() -> f32 {
    ANALYSIS_SAMPLE_RATE as f32 / HOP_SIZE as f32
}

pub struct FrontEnd {
    stft: Stft,
    magnitude_scale: f32,
    flux_bins: (usize, usize),
    /// Half-open bin range per timbre band.
    band_bins: [(usize, usize); TIMBRE_BANDS],
    previous: Vec<f32>,
    has_previous: bool,
    /// All-band onset strength, one value per frame since creation.
    pub flux: Vec<f32>,
    band_sums: [f32; TIMBRE_BANDS],
    band_frames: u32,
}

impl FrontEnd {
    pub fn new() -> Self {
        let stft = Stft::new(WINDOW_SIZE, HOP_SIZE);
        let bin_of = |hz: f32| (hz * WINDOW_SIZE as f32 / ANALYSIS_SAMPLE_RATE as f32) as usize;
        let ratio = (HIGH_HZ / LOW_HZ).powf(1.0 / TIMBRE_BANDS as f32);
        let band_bins = std::array::from_fn(|band| {
            let low = bin_of(LOW_HZ * ratio.powi(band as i32));
            let high = bin_of(LOW_HZ * ratio.powi(band as i32 + 1)).max(low + 1);
            (low, high)
        });
        Self {
            previous: vec![0.0; stft.bin_count()],
            stft,
            magnitude_scale: 4.0 / WINDOW_SIZE as f32,
            flux_bins: (bin_of(30.0), bin_of(11_000.0)),
            band_bins,
            has_previous: false,
            flux: Vec::new(),
            band_sums: [0.0; TIMBRE_BANDS],
            band_frames: 0,
        }
    }

    pub fn push(&mut self, samples: &[f32]) {
        let Self {
            stft,
            magnitude_scale,
            flux_bins,
            band_bins,
            previous,
            has_previous,
            flux,
            band_sums,
            band_frames,
        } = self;
        stft.push(samples, |magnitudes| {
            let mut rise = 0.0;
            for (bin, (&magnitude, previous)) in
                magnitudes.iter().zip(previous.iter_mut()).enumerate()
            {
                let log_magnitude = (LOG_COMPRESSION * *magnitude_scale * magnitude).ln_1p();
                if (flux_bins.0..flux_bins.1).contains(&bin) {
                    rise += (log_magnitude - *previous).max(0.0);
                }
                *previous = log_magnitude;
            }
            for (sum, &(low, high)) in band_sums.iter_mut().zip(band_bins.iter()) {
                let band = &previous[low..high];
                *sum += band.iter().sum::<f32>() / band.len() as f32;
            }
            *band_frames += 1;
            if !*has_previous {
                *has_previous = true;
                flux.push(0.0);
                return;
            }
            flux.push(rise / (flux_bins.1 - flux_bins.0) as f32);
        });
    }

    /// Mean log band energies since the last call.
    pub fn take_timbre(&mut self) -> [f32; TIMBRE_BANDS] {
        let frames = self.band_frames.max(1) as f32;
        let timbre = self.band_sums.map(|sum| sum / frames);
        self.band_sums = [0.0; TIMBRE_BANDS];
        self.band_frames = 0;
        timbre
    }
}

/// The onset envelope of a whole clip.
pub fn flux_of(samples: &[f32]) -> Vec<f32> {
    let mut front_end = FrontEnd::new();
    front_end.push(samples);
    front_end.flux
}
