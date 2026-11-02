use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::CaptureError;

const CAPTURE_THREAD_NAME: &str = "hunch-capture";

/// The format a source delivers, which the OS picks, not the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamFormat {
    pub sample_rate: u32,
    pub channels: u16,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StreamEvent {
    /// Interleaved f32 samples in the stream's format, whole frames only.
    Frames(Vec<f32>),
    /// The stream stopped. Nothing follows it.
    Ended(EndReason),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EndReason {
    /// The source went away: the app closed or the device was unplugged or disabled.
    SourceGone,
    /// Capture failed partway through.
    Failed(String),
}

/// A running capture. Audio is read on its own thread; dropping the stream stops it.
pub struct CaptureStream {
    format: StreamFormat,
    events: Receiver<StreamEvent>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl CaptureStream {
    /// Runs `capture` on a new thread and waits for it to call [`PendingStream::started`] or
    /// [`PendingStream::failed`]. OS handles are opened inside `capture`, so they never cross
    /// threads.
    pub fn spawn<F>(capture: F) -> Result<Self, CaptureError>
    where
        F: FnOnce(PendingStream) + Send + 'static,
    {
        let (ready_sender, ready) = mpsc::sync_channel(1);
        let (event_sender, events) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let pending = PendingStream {
            ready: ready_sender,
            sink: StreamSink {
                events: event_sender,
                stop: Arc::clone(&stop),
            },
        };
        let worker = thread::Builder::new()
            .name(CAPTURE_THREAD_NAME.to_owned())
            .spawn(move || capture(pending))
            .map_err(|error| {
                CaptureError::Backend(format!("cannot start capture thread: {error}"))
            })?;

        let start = ready.recv().unwrap_or_else(|_| {
            Err(CaptureError::Backend(
                "capture thread exited before starting".to_owned(),
            ))
        });
        match start {
            Ok(format) => Ok(Self {
                format,
                events,
                stop,
                worker: Some(worker),
            }),
            Err(error) => {
                // The thread is returning after a failed start; don't leave it detached.
                let _ = worker.join();
                Err(error)
            }
        }
    }

    pub fn format(&self) -> StreamFormat {
        self.format
    }

    /// Waits up to `timeout` for the next event. `None` means nothing arrived in time.
    pub fn next_event(&self, timeout: Duration) -> Option<StreamEvent> {
        match self.events.recv_timeout(timeout) {
            Ok(event) => Some(event),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => Some(StreamEvent::Ended(EndReason::Failed(
                "capture thread stopped without saying why".to_owned(),
            ))),
        }
    }
}

impl Drop for CaptureStream {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            // A panic on the capture thread has nothing left to report to.
            let _ = worker.join();
        }
    }
}

/// Handed to a capture thread until it reports whether the source opened.
pub struct PendingStream {
    ready: SyncSender<Result<StreamFormat, CaptureError>>,
    sink: StreamSink,
}

impl PendingStream {
    pub fn started(self, format: StreamFormat) -> StreamSink {
        // If `spawn` stopped waiting, the sink sees it through `is_stopped`.
        let _ = self.ready.send(Ok(format));
        self.sink
    }

    pub fn failed(self, error: CaptureError) {
        let _ = self.ready.send(Err(error));
    }
}

/// Where a capture thread sends audio once started.
pub struct StreamSink {
    events: Sender<StreamEvent>,
    stop: Arc<AtomicBool>,
}

impl StreamSink {
    /// True once the stream was dropped. The capture thread should return.
    pub fn is_stopped(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    pub fn send_frames(&self, samples: Vec<f32>) {
        let _ = self.events.send(StreamEvent::Frames(samples));
    }

    pub fn end(self, reason: EndReason) {
        let _ = self.events.send(StreamEvent::Ended(reason));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FORMAT: StreamFormat = StreamFormat {
        sample_rate: 48_000,
        channels: 2,
    };
    const TIMEOUT: Duration = Duration::from_secs(5);

    #[test]
    fn delivers_frames_then_the_end() {
        let stream = CaptureStream::spawn(|pending| {
            let sink = pending.started(FORMAT);
            sink.send_frames(vec![0.5, -0.5]);
            sink.end(EndReason::SourceGone);
        })
        .expect("stream starts");

        assert_eq!(stream.format(), FORMAT);
        assert_eq!(
            stream.next_event(TIMEOUT),
            Some(StreamEvent::Frames(vec![0.5, -0.5]))
        );
        assert_eq!(
            stream.next_event(TIMEOUT),
            Some(StreamEvent::Ended(EndReason::SourceGone))
        );
    }

    #[test]
    fn reports_a_failed_start() {
        let error = CaptureError::Backend("no device".to_owned());
        let reported = error.clone();
        let result = CaptureStream::spawn(move |pending| pending.failed(reported));

        assert_eq!(result.err(), Some(error));
    }

    #[test]
    fn reports_a_thread_that_never_starts() {
        let result = CaptureStream::spawn(drop);

        assert!(matches!(result, Err(CaptureError::Backend(_))));
    }

    #[test]
    fn dropping_the_stream_stops_the_capture_thread() {
        let stream = CaptureStream::spawn(|pending| {
            let sink = pending.started(FORMAT);
            while !sink.is_stopped() {
                thread::sleep(Duration::from_millis(1));
            }
        })
        .expect("stream starts");

        drop(stream);
    }
}
