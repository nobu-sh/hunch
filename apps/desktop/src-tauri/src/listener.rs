use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use hunch_core::Session;
use hunch_core::contract::SessionEventDto;
use tauri::{AppHandle, Emitter};

use crate::feed::AudioFeed;

/// The Tauri event that carries each `SessionEventDto` to the webview.
pub const SESSION_EVENT: &str = "session-event";

/// Runs one session at a time on its own thread, fed by an `AudioFeed`.
#[derive(Default)]
pub struct Listener {
    running: Mutex<Option<Running>>,
}

struct Running {
    should_stop: Arc<AtomicBool>,
    thread: JoinHandle<()>,
}

pub struct Restart {
    pub app: AppHandle,
    pub feed: Box<dyn AudioFeed>,
}

impl Listener {
    /// Stops the current session, if any, and starts a new one from `feed`.
    pub fn restart(&self, Restart { app, feed }: Restart) {
        let mut running = self
            .running
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(previous) = running.take() {
            previous.should_stop.store(true, Ordering::Relaxed);
            if previous.thread.join().is_err() {
                eprintln!("hunch: the previous listener thread panicked");
            }
        }

        let should_stop = Arc::new(AtomicBool::new(false));
        let thread = thread::spawn({
            let should_stop = Arc::clone(&should_stop);
            move || {
                listen(Listen {
                    app,
                    feed,
                    should_stop,
                })
            }
        });
        *running = Some(Running {
            should_stop,
            thread,
        });
    }
}

struct Listen {
    app: AppHandle,
    feed: Box<dyn AudioFeed>,
    should_stop: Arc<AtomicBool>,
}

fn listen(
    Listen {
        app,
        mut feed,
        should_stop,
    }: Listen,
) {
    let mut session = Session::new(feed.sample_rate());
    while !should_stop.load(Ordering::Relaxed) {
        let Some(chunk) = feed.next_chunk() else {
            return;
        };
        for event in session.push(&chunk) {
            if let Err(error) = app.emit(SESSION_EVENT, SessionEventDto::from(event)) {
                eprintln!("hunch: couldn't send a session event to the UI: {error}");
            }
        }
    }
}
