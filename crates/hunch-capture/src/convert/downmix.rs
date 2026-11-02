/// Interleaved samples and how many channels each frame has.
#[derive(Debug, Clone, Copy)]
pub struct Interleaved<'a> {
    pub samples: &'a [f32],
    pub channels: u16,
}

/// Averages each frame's channels into one sample. A trailing partial frame is dropped.
pub fn downmix(input: Interleaved<'_>) -> Vec<f32> {
    let channels = usize::from(input.channels.max(1));
    if channels == 1 {
        return input.samples.to_vec();
    }
    let scale = 1.0 / channels as f32;
    input
        .samples
        .chunks_exact(channels)
        .map(|frame| frame.iter().sum::<f32>() * scale)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mono_passes_through() {
        let samples = [0.1, -0.2, 0.3];
        let mono = downmix(Interleaved {
            samples: &samples,
            channels: 1,
        });

        assert_eq!(mono, samples);
    }

    #[test]
    fn stereo_averages_left_and_right() {
        let samples = [1.0, 0.0, 0.5, 0.5, -1.0, 1.0];
        let mono = downmix(Interleaved {
            samples: &samples,
            channels: 2,
        });

        assert_eq!(mono, [0.5, 0.5, 0.0]);
    }

    #[test]
    fn surround_averages_every_channel() {
        let samples = [0.6, 0.6, 0.6, 0.0, 0.0, 0.0];
        let mono = downmix(Interleaved {
            samples: &samples,
            channels: 6,
        });

        assert_eq!(mono.len(), 1);
        assert!((mono[0] - 0.3).abs() < 1e-6, "got {}", mono[0]);
    }

    #[test]
    fn drops_a_partial_frame() {
        let samples = [0.2, 0.4, 0.9];
        let mono = downmix(Interleaved {
            samples: &samples,
            channels: 2,
        });

        assert_eq!(mono.len(), 1);
    }
}
