use std::time::{Duration, Instant};

use hunch_capture::{
    ANALYSIS_SAMPLE_RATE, AnalysisFeed, AudioBackend, EndReason, SourceId, StreamEvent,
};
use hunch_core::{Session, SessionEvent};

use crate::print;

/// How often the loop wakes without audio, to check the time limit.
const POLL_INTERVAL: Duration = Duration::from_millis(100);

pub struct ListenArgs<'a> {
    pub backend: &'a dyn AudioBackend,
    pub id: SourceId,
    pub seconds: Option<f32>,
}

pub fn run(args: ListenArgs<'_>) -> Result<(), String> {
    let name = source_name(&args);
    let stream = args
        .backend
        .open(&args.id)
        .map_err(|error| error.to_string())?;
    let format = stream.format();
    println!(
        "listening to {name} ({} Hz, {} ch)",
        format.sample_rate, format.channels
    );

    let mut feed = AnalysisFeed::new(format).map_err(|error| error.to_string())?;
    let mut session = Session::new(ANALYSIS_SAMPLE_RATE);
    let mut summary = Summary::default();
    let started = Instant::now();
    let deadline = args
        .seconds
        .map(|seconds| started + Duration::from_secs_f32(seconds));

    loop {
        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            println!("time limit reached");
            break;
        }
        match stream.next_event(POLL_INTERVAL) {
            None => {}
            Some(StreamEvent::Frames(samples)) => {
                let mut events = Vec::new();
                feed.push(&samples, |chunk| events.extend(session.push(chunk)))
                    .map_err(|error| error.to_string())?;
                for event in &events {
                    summary.record(event);
                    print::session_event(print::TimedEvent {
                        elapsed: started.elapsed(),
                        event,
                    });
                }
            }
            Some(StreamEvent::Ended(EndReason::SourceGone)) => {
                println!("source ended: the app closed or the device went away");
                break;
            }
            Some(StreamEvent::Ended(EndReason::Failed(message))) => {
                return Err(format!("capture failed: {message}"));
            }
        }
    }
    summary.print();
    Ok(())
}

/// The source's name from the listing, or its id when it isn't listed (an app without a session).
fn source_name(args: &ListenArgs<'_>) -> String {
    let sources = args.backend.list_sources().unwrap_or_default();
    sources
        .into_iter()
        .find(|source| source.id == args.id)
        .map_or_else(|| args.id.to_string(), |source| source.name)
}

#[derive(Default)]
struct Summary {
    windows: u32,
    estimates: u32,
}

impl Summary {
    fn record(&mut self, event: &SessionEvent) {
        match event {
            SessionEvent::WindowStarted { .. } => self.windows += 1,
            SessionEvent::Estimate { .. } => self.estimates += 1,
            SessionEvent::WindowEnded { .. } => {}
        }
    }

    fn print(&self) {
        if self.windows == 0 {
            println!("no windows: the source stayed silent");
            return;
        }
        println!("{} window(s), {} estimate(s)", self.windows, self.estimates);
    }
}
