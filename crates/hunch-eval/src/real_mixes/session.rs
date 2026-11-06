//! Streams a recorded WAV through a `hunch-core` `Session`, without loading it whole.

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use hound::{SampleFormat, WavReader};
use hunch_core::{Session, SessionEvent};

use crate::real_mixes::score::TimedEvent;
use crate::resample::ANALYSIS_SAMPLE_RATE;
use crate::run;

pub struct SessionRun {
    pub events: Vec<TimedEvent>,
    pub audio_seconds: f32,
}

/// Runs a session over a WAV at the analysis rate (what `hunch-capture listen --record` writes),
/// averaging any channels to mono.
pub fn run_session(path: &Path) -> Result<SessionRun, String> {
    let reader = WavReader::open(path)
        .map_err(|error| format!("cannot open {}: {error}", path.display()))?;
    let spec = reader.spec();
    if spec.sample_rate != ANALYSIS_SAMPLE_RATE {
        return Err(format!(
            "{} is {} Hz; record with `hunch-capture listen --record` or convert it to {ANALYSIS_SAMPLE_RATE} Hz",
            path.display(),
            spec.sample_rate
        ));
    }
    let channels = usize::from(spec.channels.max(1));
    let read_error = |error: hound::Error| format!("cannot read {}: {error}", path.display());

    let mut feed = SessionFeed::new();
    let mut frame = Vec::with_capacity(channels);
    for sample in samples(reader) {
        frame.push(sample.map_err(read_error)?);
        if frame.len() == channels {
            feed.push(frame.iter().sum::<f32>() / channels as f32);
            frame.clear();
        }
    }
    Ok(feed.finish())
}

/// The file's samples as f32 in [-1, 1], whatever their stored format.
fn samples(reader: WavReader<BufReader<File>>) -> Box<dyn Iterator<Item = hound::Result<f32>>> {
    let spec = reader.spec();
    match spec.sample_format {
        SampleFormat::Float => Box::new(reader.into_samples::<f32>()),
        SampleFormat::Int => {
            let full_scale = 2f32.powi(i32::from(spec.bits_per_sample) - 1);
            Box::new(
                reader
                    .into_samples::<i32>()
                    .map(move |sample| sample.map(|value| value as f32 / full_scale)),
            )
        }
    }
}

/// Pushes mono samples to a session in the extension's chunk size, timing each event.
struct SessionFeed {
    session: Session,
    chunk: Vec<f32>,
    chunk_len: usize,
    samples_pushed: u64,
    events: Vec<TimedEvent>,
}

impl SessionFeed {
    fn new() -> Self {
        let chunk_len = run::chunk_len();
        Self {
            session: Session::new(ANALYSIS_SAMPLE_RATE),
            chunk: Vec::with_capacity(chunk_len),
            chunk_len,
            samples_pushed: 0,
            events: Vec::new(),
        }
    }

    fn push(&mut self, sample: f32) {
        self.chunk.push(sample);
        if self.chunk.len() == self.chunk_len {
            self.flush_chunk();
        }
    }

    fn flush_chunk(&mut self) {
        let events = self.session.push(&self.chunk);
        self.samples_pushed += self.chunk.len() as u64;
        self.chunk.clear();
        self.record(events);
    }

    fn record(&mut self, events: Vec<SessionEvent>) {
        let emitted_at = self.samples_pushed as f32 / ANALYSIS_SAMPLE_RATE as f32;
        self.events.extend(
            events
                .into_iter()
                .map(|event| TimedEvent { emitted_at, event }),
        );
    }

    fn finish(mut self) -> SessionRun {
        if !self.chunk.is_empty() {
            self.flush_chunk();
        }
        let events = self.session.finish();
        self.record(events);
        SessionRun {
            events: self.events,
            audio_seconds: self.samples_pushed as f32 / ANALYSIS_SAMPLE_RATE as f32,
        }
    }
}
