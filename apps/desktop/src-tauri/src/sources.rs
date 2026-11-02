//! What the app can listen to: capture sources plus, in debug builds, the synthesized test beat.

use hunch_capture::{Source, SourceKind};
use serde::Serialize;

/// The test beat's id in `list_sources`. Debug builds only.
pub const DEV_SOURCE_ID: &str = "dev:test-beat";
const DEV_SOURCE_NAME: &str = "Test beat (dev)";
/// Whether this build offers the dev source.
pub const HAS_DEV_SOURCE: bool = cfg!(debug_assertions);

/// A source as the UI sees it, from `list_sources` and `current_source`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceDto {
    pub id: String,
    pub kind: SourceKindDto,
    pub name: String,
    pub is_default: bool,
    pub is_output: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SourceKindDto {
    Device,
    App,
}

/// What `list_sources` returns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceListDto {
    pub sources: Vec<SourceDto>,
    /// False when this platform can't list apps, so the UI hides its app list.
    pub lists_apps: bool,
}

/// Something a session can listen to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Listenable {
    TestBeat,
    Capture(Source),
}

impl Listenable {
    pub fn id(&self) -> &str {
        match self {
            Listenable::TestBeat => DEV_SOURCE_ID,
            Listenable::Capture(source) => source.id.as_str(),
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Listenable::TestBeat => DEV_SOURCE_NAME,
            Listenable::Capture(source) => &source.name,
        }
    }

    pub fn is_app(&self) -> bool {
        matches!(self, Listenable::Capture(source) if source.kind == SourceKind::App)
    }

    /// Apps and output devices should carry sound; a mic or the test beat may rightly be quiet
    /// or can't be.
    pub fn watches_for_silence(&self) -> bool {
        match self {
            Listenable::TestBeat => false,
            Listenable::Capture(source) => source.kind == SourceKind::App || source.is_output,
        }
    }

    pub fn to_dto(&self) -> SourceDto {
        match self {
            Listenable::TestBeat => SourceDto {
                id: DEV_SOURCE_ID.to_owned(),
                kind: SourceKindDto::Device,
                name: DEV_SOURCE_NAME.to_owned(),
                is_default: false,
                is_output: false,
            },
            Listenable::Capture(source) => SourceDto {
                id: source.id.to_string(),
                kind: match source.kind {
                    SourceKind::Device => SourceKindDto::Device,
                    SourceKind::App => SourceKindDto::App,
                },
                name: source.name.clone(),
                is_default: source.is_default,
                is_output: source.is_output,
            },
        }
    }
}

pub struct Available {
    pub sources: Vec<Source>,
    pub includes_dev_source: bool,
}

/// Every capture source, then the dev source when it's included.
pub fn listenables(available: Available) -> Vec<Listenable> {
    let mut listenables: Vec<Listenable> = available
        .sources
        .into_iter()
        .map(Listenable::Capture)
        .collect();
    if available.includes_dev_source {
        listenables.push(Listenable::TestBeat);
    }
    listenables
}

pub fn find<'a>(listenables: &'a [Listenable], id: &str) -> Option<&'a Listenable> {
    listenables.iter().find(|listenable| listenable.id() == id)
}

pub struct StartChoice<'a> {
    pub listenables: &'a [Listenable],
    /// The id the user last selected, if one was saved.
    pub remembered: Option<&'a str>,
    /// `HUNCH_DEV_SOURCE=test-beat` asked for the test beat.
    pub wants_dev_source: bool,
}

/// What `listen` starts: the dev source when asked for, else the remembered source while it's
/// still around, else the default output device. `None` when none of those exist.
pub fn choose_start(choice: StartChoice<'_>) -> Option<Listenable> {
    if choice.wants_dev_source {
        if let Some(dev) = find(choice.listenables, DEV_SOURCE_ID) {
            return Some(dev.clone());
        }
    }
    if let Some(remembered) = choice
        .remembered
        .and_then(|id| find(choice.listenables, id))
    {
        return Some(remembered.clone());
    }
    choice
        .listenables
        .iter()
        .find(|listenable| is_default_output(listenable))
        .cloned()
}

fn is_default_output(listenable: &Listenable) -> bool {
    matches!(
        listenable,
        Listenable::Capture(source)
            if source.kind == SourceKind::Device && source.is_output && source.is_default
    )
}

#[cfg(test)]
mod tests;
