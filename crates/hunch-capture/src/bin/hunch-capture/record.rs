use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use hound::{SampleFormat, WavSpec, WavWriter};
use hunch_capture::ANALYSIS_SAMPLE_RATE;

/// The header is rewritten this often, so a recording stopped with Ctrl+C stays readable and
/// loses at most this much audio.
const FLUSH_EVERY_SECONDS: u64 = 10;
const SPEC: WavSpec = WavSpec {
    channels: 1,
    sample_rate: ANALYSIS_SAMPLE_RATE,
    bits_per_sample: 32,
    sample_format: SampleFormat::Float,
};

/// Streams the analysis chunks to a mono 32-bit float WAV at the analysis rate.
pub struct Recorder {
    writer: WavWriter<BufWriter<File>>,
    samples_since_flush: u64,
}

impl Recorder {
    pub fn create(path: &Path) -> Result<Self, String> {
        let writer = WavWriter::create(path, SPEC)
            .map_err(|error| format!("cannot create {}: {error}", path.display()))?;
        Ok(Self {
            writer,
            samples_since_flush: 0,
        })
    }

    pub fn write(&mut self, chunk: &[f32]) -> Result<(), String> {
        for &sample in chunk {
            self.writer
                .write_sample(sample)
                .map_err(|error| format!("cannot write the recording: {error}"))?;
        }
        self.samples_since_flush += chunk.len() as u64;
        if self.samples_since_flush >= FLUSH_EVERY_SECONDS * u64::from(ANALYSIS_SAMPLE_RATE) {
            self.writer
                .flush()
                .map_err(|error| format!("cannot write the recording: {error}"))?;
            self.samples_since_flush = 0;
        }
        Ok(())
    }

    pub fn finish(self) -> Result<(), String> {
        self.writer
            .finalize()
            .map_err(|error| format!("cannot finish the recording: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_chunks_as_a_mono_wav_at_the_analysis_rate() {
        let path = std::env::temp_dir().join("hunch-capture-record-test.wav");
        let mut recorder = Recorder::create(&path).expect("create");
        recorder.write(&[0.25, -0.5]).expect("write");
        recorder.write(&[0.75]).expect("write");
        recorder.finish().expect("finish");

        let mut reader = hound::WavReader::open(&path).expect("open");
        let samples: Vec<f32> = reader.samples().map(|s| s.expect("sample")).collect();
        let spec = reader.spec();
        std::fs::remove_file(&path).ok();

        assert_eq!(spec, SPEC);
        assert_eq!(samples, vec![0.25, -0.5, 0.75]);
    }
}
