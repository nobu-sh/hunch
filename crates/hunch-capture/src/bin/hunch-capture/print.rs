use std::time::Duration;

use hunch_capture::{Source, SourceKind};
use hunch_core::{Estimate, SessionEvent};

pub fn sources(sources: &[Source]) {
    let devices = sources
        .iter()
        .filter(|source| source.kind == SourceKind::Device);
    let apps = sources
        .iter()
        .filter(|source| source.kind == SourceKind::App);
    println!("devices:");
    for source in devices {
        let direction = if source.is_output { "output" } else { "input" };
        let default = if source.is_default { ", default" } else { "" };
        println!("  {}  [{direction}{default}]  {}", source.name, source.id);
    }
    println!("apps:");
    for source in apps {
        println!("  {}  {}", source.name, source.id);
    }
}

pub struct TimedEvent<'a> {
    /// Wall-clock time since listening started.
    pub elapsed: Duration,
    pub event: &'a SessionEvent,
}

pub fn session_event(timed: TimedEvent<'_>) {
    let at = timed.elapsed.as_secs_f32();
    match timed.event {
        SessionEvent::WindowStarted {
            window_id,
            at_seconds,
        } => println!("[{at:6.1}s] window {window_id} started at {at_seconds:.1} s of audio"),
        SessionEvent::Estimate {
            window_id,
            estimate,
        } => println!(
            "[{at:6.1}s] window {window_id} estimate: {}",
            describe(estimate)
        ),
        SessionEvent::WindowEnded {
            window_id,
            estimate,
            duration_seconds,
            too_short,
        } => {
            let result = estimate.as_ref().map_or("no estimate".to_owned(), describe);
            let short = if *too_short { " (too short)" } else { "" };
            println!(
                "[{at:6.1}s] window {window_id} ended after {duration_seconds:.1} s{short}: {result}"
            );
        }
    }
}

fn describe(estimate: &Estimate) -> String {
    let key = estimate
        .key
        .map_or("no key".to_owned(), |key| key.to_string());
    let bpm = estimate
        .bpm
        .map_or("no BPM".to_owned(), |bpm| format!("{bpm:.1} BPM"));
    format!(
        "key {key} ({:.2}), {bpm} ({:.2}), {:.1} s analyzed",
        estimate.key_confidence, estimate.bpm_confidence, estimate.seconds_analyzed
    )
}
