//! Audio capture from devices and apps, converted into what `hunch-core` analyzes.
//!
//! Everything public is platform-independent. OS capture APIs stay inside `platform`.

mod backend;
mod convert;
mod error;
mod platform;
mod source;
mod stream;

pub use backend::AudioBackend;
pub use convert::{ANALYSIS_SAMPLE_RATE, AnalysisFeed, CHUNK_MILLISECONDS, CHUNK_SAMPLES};
pub use error::CaptureError;
pub use platform::default_backend;
pub use source::{Source, SourceId, SourceKind};
pub use stream::{CaptureStream, EndReason, PendingStream, StreamEvent, StreamFormat, StreamSink};
