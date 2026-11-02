//! Real capture as an `AudioFeed`: a `CaptureStream` converted by an `AnalysisFeed`.

use std::time::Duration;

use hunch_capture::{
    ANALYSIS_SAMPLE_RATE, AnalysisFeed, CaptureError, CaptureStream, EndReason, StreamEvent,
};

use crate::feed::{AudioFeed, FeedEnd, FeedEvent};

/// How long `next` waits for audio, so the listener notices a stop while the source is quiet.
const WAIT_FOR_AUDIO: Duration = Duration::from_millis(100);

pub struct CaptureFeed {
    stream: CaptureStream,
    analysis: AnalysisFeed,
}

impl CaptureFeed {
    pub fn new(stream: CaptureStream) -> Result<Self, CaptureError> {
        let analysis = AnalysisFeed::new(stream.format())?;
        Ok(Self { stream, analysis })
    }
}

impl AudioFeed for CaptureFeed {
    fn sample_rate(&self) -> u32 {
        ANALYSIS_SAMPLE_RATE
    }

    fn next(&mut self) -> FeedEvent {
        match self.stream.next_event(WAIT_FOR_AUDIO) {
            None => FeedEvent::Waiting,
            Some(StreamEvent::Frames(interleaved)) => {
                let mut mono = Vec::new();
                match self
                    .analysis
                    .push(&interleaved, |chunk| mono.extend_from_slice(chunk))
                {
                    Ok(()) => FeedEvent::Audio(mono),
                    Err(error) => FeedEvent::Ended(FeedEnd::Failed(error.to_string())),
                }
            }
            Some(StreamEvent::Ended(EndReason::SourceGone)) => {
                FeedEvent::Ended(FeedEnd::SourceGone)
            }
            Some(StreamEvent::Ended(EndReason::Failed(message))) => {
                FeedEvent::Ended(FeedEnd::Failed(message))
            }
        }
    }
}
