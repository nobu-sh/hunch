use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::Instant;

use hunch_core::contract::SessionEventDto;
use hunch_core::{Session, SessionEvent};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::feed::{AudioFeed, FeedEnd, FeedEvent};
use crate::silence::{Observation, SilenceChange, SilenceDetector};
use crate::sources::{Listenable, SourceDto};
use crate::status::{Failure, SOURCE_STATUS_EVENT, SourceStatusDto};

/// The Tauri event that carries each `SessionEventDto` to the webview.
pub const SESSION_EVENT: &str = "session-event";

/// Runs one session at a time on its own thread, fed by an `AudioFeed`.
#[derive(Default)]
pub struct Listener {
    running: Mutex<Option<Running>>,
}

struct Running {
    source: Listenable,
    should_stop: Arc<AtomicBool>,
    thread: JoinHandle<()>,
}

pub struct Restart {
    pub app: AppHandle,
    pub feed: Box<dyn AudioFeed>,
    pub source: Listenable,
}

impl Listener {
    /// Stops the current session, if any, once it has finished its open window, then starts a new
    /// one from `feed`.
    pub fn restart(&self, Restart { app, feed, source }: Restart) {
        let mut running = self.lock();
        if let Some(previous) = running.take() {
            previous.should_stop.store(true, Ordering::Relaxed);
            if previous.thread.join().is_err() {
                eprintln!("hunch: the previous listener thread panicked");
            }
        }

        let should_stop = Arc::new(AtomicBool::new(false));
        let thread = thread::spawn({
            let should_stop = Arc::clone(&should_stop);
            let source = source.clone();
            move || {
                listen(Listen {
                    app,
                    feed,
                    source,
                    should_stop,
                })
            }
        });
        *running = Some(Running {
            source,
            should_stop,
            thread,
        });
    }

    /// The source the latest session started on, even if it has since ended.
    pub fn current_source(&self) -> Option<SourceDto> {
        self.lock().as_ref().map(|running| running.source.to_dto())
    }

    fn lock(&self) -> MutexGuard<'_, Option<Running>> {
        self.running
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

struct Listen {
    app: AppHandle,
    feed: Box<dyn AudioFeed>,
    source: Listenable,
    should_stop: Arc<AtomicBool>,
}

/// Emits `source-status` `listening`, then the session's events as audio arrives. When the feed
/// ends or the session is replaced, the open window is finished first, so its last `session-event`s
/// (ending in `windowEnded`) come before the `source-status` `ended` or `failed`, or before the
/// next session's `listening`.
fn listen(
    Listen {
        app,
        mut feed,
        source,
        should_stop,
    }: Listen,
) {
    let mut session = Session::new(feed.sample_rate());
    let mut silence = SilenceDetector::new(source.watches_for_silence());
    let started = Instant::now();
    emit(
        &app,
        SOURCE_STATUS_EVENT,
        SourceStatusDto::listening(&source),
    );

    let end = loop {
        if should_stop.load(Ordering::Relaxed) {
            break None;
        }
        let samples = match feed.next() {
            FeedEvent::Audio(samples) => samples,
            FeedEvent::Waiting => Vec::new(),
            FeedEvent::Ended(end) => break Some(end),
        };
        let change = silence.observe(Observation {
            elapsed: started.elapsed(),
            samples: &samples,
        });
        if let Some(change) = change {
            emit(&app, SOURCE_STATUS_EVENT, silence_status(&source, change));
        }
        emit_session_events(&app, session.push(&samples));
    };

    emit_session_events(&app, session.finish());
    if let Some(end) = end {
        emit(&app, SOURCE_STATUS_EVENT, end_status(&source, end));
    }
}

fn emit_session_events(app: &AppHandle, events: Vec<SessionEvent>) {
    for event in events {
        emit(app, SESSION_EVENT, SessionEventDto::from(event));
    }
}

fn end_status(source: &Listenable, end: FeedEnd) -> SourceStatusDto {
    match end {
        FeedEnd::SourceGone => SourceStatusDto::ended(source),
        FeedEnd::Failed(error) => SourceStatusDto::failed(Failure {
            source: Some(source),
            message: &format!("Capture from {} stopped: {error}", source.name()),
        }),
    }
}

fn silence_status(source: &Listenable, change: SilenceChange) -> SourceStatusDto {
    match change {
        SilenceChange::BecameSilent => SourceStatusDto::silent(source),
        SilenceChange::HeardAudio => SourceStatusDto::listening(source),
    }
}

pub fn emit(app: &AppHandle, event: &str, payload: impl Serialize + Clone) {
    if let Err(error) = app.emit(event, payload) {
        eprintln!("hunch: couldn't send a {event} event to the UI: {error}");
    }
}
