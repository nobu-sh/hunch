//! The Tauri commands the UI calls: list sources, pick one, see which is current, and listen.

use hunch_capture::{AudioBackend, CaptureError};
use tauri::{AppHandle, State};

use crate::capture_feed::CaptureFeed;
use crate::feed::AudioFeed;
use crate::listener::{self, Listener, emit};
use crate::remembered::RememberedSourceFile;
use crate::sources::{
    self, Available, HAS_DEV_SOURCE, Listenable, SourceDto, SourceListDto, StartChoice,
};
use crate::status::{Failure, SOURCE_STATUS_EVENT, SourceStatusDto};
use crate::test_beat::TestBeatFeed;

/// Set to `test-beat` in a debug build to make `listen` start the test beat.
const DEV_SOURCE_VARIABLE: &str = "HUNCH_DEV_SOURCE";
const DEV_SOURCE_VALUE: &str = "test-beat";
const NO_SOURCE_MESSAGE: &str = "There's no output device to listen to.";

pub struct AppState {
    pub backend: Box<dyn AudioBackend>,
    pub listener: Listener,
    pub remembered: RememberedSourceFile,
}

#[tauri::command]
pub async fn list_sources(state: State<'_, AppState>) -> Result<SourceListDto, String> {
    let sources = available(&state)
        .map_err(|error| error.to_string())?
        .iter()
        .map(Listenable::to_dto)
        .collect();
    Ok(SourceListDto {
        sources,
        lists_apps: state.backend.lists_apps(),
    })
}

/// Starts a fresh session on `id` and remembers it. The current session keeps running if the
/// new source can't start.
#[tauri::command]
pub async fn select_source(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let listenables = available(&state).map_err(|error| error.to_string())?;
    let source = sources::find(&listenables, &id)
        .ok_or_else(|| format!("There's no audio source with id {id} any more."))?
        .clone();
    start(Start {
        app,
        state: &state,
        source: &source,
    })?;
    if let Err(error) = state.remembered.save(source.id()) {
        eprintln!("hunch: couldn't remember the selected source: {error}");
    }
    Ok(())
}

#[tauri::command]
pub fn current_source(state: State<'_, AppState>) -> Option<SourceDto> {
    state.listener.current_source()
}

/// Starts listening from scratch. The UI calls it once it's subscribed, so it sees every event.
#[tauri::command]
pub async fn listen(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let Err(failure) = start_chosen(&app, &state) else {
        return Ok(());
    };
    eprintln!("hunch: couldn't start listening: {}", failure.message);
    let status = SourceStatusDto::failed(Failure {
        source: failure.source.as_ref(),
        message: &failure.message,
    });
    emit(&app, SOURCE_STATUS_EVENT, status);
    Err(failure.message)
}

struct StartFailure {
    source: Option<Listenable>,
    message: String,
}

/// Starts what `sources::choose_start` picks.
fn start_chosen(app: &AppHandle, state: &AppState) -> Result<(), StartFailure> {
    let without_source = |message: String| StartFailure {
        source: None,
        message,
    };
    let listenables = available(state).map_err(|error| without_source(error.to_string()))?;
    let remembered = state.remembered.load();
    let source = sources::choose_start(StartChoice {
        listenables: &listenables,
        remembered: remembered.as_deref(),
        wants_dev_source: wants_dev_source(),
    })
    .ok_or_else(|| without_source(NO_SOURCE_MESSAGE.to_owned()))?;
    start(Start {
        app: app.clone(),
        state,
        source: &source,
    })
    .map_err(|message| StartFailure {
        source: Some(source.clone()),
        message,
    })
}

struct Start<'a> {
    app: AppHandle,
    state: &'a AppState,
    source: &'a Listenable,
}

fn start(Start { app, state, source }: Start<'_>) -> Result<(), String> {
    let feed = open_feed(state.backend.as_ref(), source)?;
    state.listener.restart(listener::Restart {
        app,
        feed,
        source: source.clone(),
    });
    Ok(())
}

fn open_feed(
    backend: &dyn AudioBackend,
    source: &Listenable,
) -> Result<Box<dyn AudioFeed>, String> {
    let Listenable::Capture(capture) = source else {
        return Ok(Box::new(TestBeatFeed::new()));
    };
    let couldnt_start =
        |error: CaptureError| format!("Couldn't listen to {}: {error}", capture.name);
    let stream = backend.open(&capture.id).map_err(couldnt_start)?;
    let feed = CaptureFeed::new(stream).map_err(couldnt_start)?;
    Ok(Box::new(feed))
}

fn available(state: &AppState) -> Result<Vec<Listenable>, CaptureError> {
    Ok(sources::listenables(Available {
        sources: state.backend.list_sources()?,
        includes_dev_source: HAS_DEV_SOURCE,
    }))
}

fn wants_dev_source() -> bool {
    let asked = std::env::var(DEV_SOURCE_VARIABLE).is_ok_and(|value| value == DEV_SOURCE_VALUE);
    HAS_DEV_SOURCE && asked
}
