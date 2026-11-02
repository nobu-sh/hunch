//! Turns captured audio in whatever format the OS delivers into what `hunch-core` analyzes.

mod chunk;
mod downmix;
mod resample;

use chunk::Chunker;
use downmix::{Interleaved, downmix};
use resample::{Rates, StreamResampler};

use crate::{CaptureError, StreamFormat};

/// The rate `hunch-core` analyzes at, the same as the extension's AudioContext.
pub const ANALYSIS_SAMPLE_RATE: u32 = 22_050;
/// How much audio each analysis chunk holds.
pub const CHUNK_MILLISECONDS: u32 = 100;
const MILLISECONDS_PER_SECOND: u32 = 1_000;
pub const CHUNK_SAMPLES: usize =
    (ANALYSIS_SAMPLE_RATE * CHUNK_MILLISECONDS / MILLISECONDS_PER_SECOND) as usize;

/// Downmixes to mono, resamples to [`ANALYSIS_SAMPLE_RATE`] and cuts [`CHUNK_SAMPLES`] chunks.
pub struct AnalysisFeed {
    channels: u16,
    resampler: StreamResampler,
    chunker: Chunker,
}

impl AnalysisFeed {
    pub fn new(format: StreamFormat) -> Result<Self, CaptureError> {
        if format.sample_rate == 0 || format.channels == 0 {
            return Err(CaptureError::Backend(format!(
                "unusable stream format: {} Hz, {} channels",
                format.sample_rate, format.channels
            )));
        }
        let resampler = StreamResampler::new(Rates {
            input: format.sample_rate,
            output: ANALYSIS_SAMPLE_RATE,
        })?;
        Ok(Self {
            channels: format.channels,
            resampler,
            chunker: Chunker::new(CHUNK_SAMPLES),
        })
    }

    /// Takes interleaved samples in the stream's format and calls `on_chunk` for each mono chunk
    /// at the analysis rate that is now complete.
    pub fn push(
        &mut self,
        interleaved: &[f32],
        on_chunk: impl FnMut(&[f32]),
    ) -> Result<(), CaptureError> {
        let mono = downmix(Interleaved {
            samples: interleaved,
            channels: self.channels,
        });
        let resampled = self.resampler.push(&mono)?;
        self.chunker.push(&resampled, on_chunk);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAPTURE_FORMAT: StreamFormat = StreamFormat {
        sample_rate: 48_000,
        channels: 2,
    };
    const PACKET_FRAMES: usize = 480;

    #[test]
    fn a_second_of_stereo_becomes_about_ten_mono_chunks() {
        let mut feed = AnalysisFeed::new(CAPTURE_FORMAT).expect("feed");
        let one_second = vec![0.25; CAPTURE_FORMAT.sample_rate as usize * 2];

        let mut chunk_lengths = Vec::new();
        for packet in one_second.chunks(PACKET_FRAMES * 2) {
            feed.push(packet, |chunk| chunk_lengths.push(chunk.len()))
                .expect("push");
        }

        // The resampler holds back up to one step, so the tenth chunk may still be pending.
        assert!((9..=10).contains(&chunk_lengths.len()), "{chunk_lengths:?}");
        assert!(chunk_lengths.iter().all(|&len| len == CHUNK_SAMPLES));
    }

    #[test]
    fn rejects_an_empty_format() {
        let format = StreamFormat {
            sample_rate: 0,
            channels: 2,
        };

        assert!(AnalysisFeed::new(format).is_err());
    }
}
