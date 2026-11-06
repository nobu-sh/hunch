//! What the app can listen to: capture sources plus, in debug builds, the synthesized dev sources.

use hunch_capture::{Source, SourceKind};
use serde::Serialize;

use crate::remembered::RememberedSource;

/// Whether this build offers the dev sources.
pub const HAS_DEV_SOURCE: bool = cfg!(debug_assertions);

/// A synthesized source for development (`test_beat`). Debug builds only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DevSource {
    /// 120 BPM in A minor.
    TestBeat,
    /// The test beat straight into 140 BPM in F# minor, looping: a track change without a pause.
    TestMix,
}

impl DevSource {
    pub const ALL: [DevSource; 2] = [DevSource::TestBeat, DevSource::TestMix];

    /// Its id in `list_sources`.
    pub fn id(self) -> &'static str {
        match self {
            DevSource::TestBeat => "dev:test-beat",
            DevSource::TestMix => "dev:test-mix",
        }
    }

    fn name(self) -> &'static str {
        match self {
            DevSource::TestBeat => "Test beat (dev)",
            DevSource::TestMix => "Test mix (dev)",
        }
    }

    /// The source a `HUNCH_DEV_SOURCE` value asks for: `test-beat` or `test-mix`.
    pub fn from_variable(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|source| source.id().strip_prefix("dev:") == Some(value))
    }
}

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
    Dev(DevSource),
    Capture(Source),
}

impl Listenable {
    pub fn id(&self) -> &str {
        match self {
            Listenable::Dev(source) => source.id(),
            Listenable::Capture(source) => source.id.as_str(),
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Listenable::Dev(source) => source.name(),
            Listenable::Capture(source) => &source.name,
        }
    }

    pub fn is_app(&self) -> bool {
        matches!(self, Listenable::Capture(source) if source.kind == SourceKind::App)
    }

    /// Apps and output devices should carry sound; a mic or a dev source may rightly be quiet
    /// or can't be.
    pub fn watches_for_silence(&self) -> bool {
        match self {
            Listenable::Dev(_) => false,
            Listenable::Capture(source) => source.kind == SourceKind::App || source.is_output,
        }
    }

    pub fn to_dto(&self) -> SourceDto {
        match self {
            Listenable::Dev(source) => SourceDto {
                id: source.id().to_owned(),
                kind: SourceKindDto::Device,
                name: source.name().to_owned(),
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

/// Every capture source, then the dev sources when they're included.
pub fn listenables(available: Available) -> Vec<Listenable> {
    let mut listenables: Vec<Listenable> = available
        .sources
        .into_iter()
        .map(Listenable::Capture)
        .collect();
    if available.includes_dev_source {
        listenables.extend(DevSource::ALL.map(Listenable::Dev));
    }
    listenables
}

pub fn find<'a>(listenables: &'a [Listenable], id: &str) -> Option<&'a Listenable> {
    listenables.iter().find(|listenable| listenable.id() == id)
}

pub struct StartChoice<'a> {
    pub listenables: &'a [Listenable],
    /// The source the user last selected, if one was saved.
    pub remembered: Option<&'a RememberedSource>,
    /// The dev source `HUNCH_DEV_SOURCE` asked for (`test-beat` or `test-mix`).
    pub wanted_dev_source: Option<DevSource>,
}

/// What `listen` starts: the dev source when asked for, else the remembered source while it's
/// still around (see `find_remembered`), else the default output device. `None` when none of
/// those exist.
pub fn choose_start(choice: StartChoice<'_>) -> Option<Listenable> {
    if let Some(dev_source) = choice.wanted_dev_source {
        if let Some(dev) = find(choice.listenables, dev_source.id()) {
            return Some(dev.clone());
        }
    }
    if let Some(remembered) = choice
        .remembered
        .and_then(|remembered| find_remembered(choice.listenables, remembered))
    {
        return Some(remembered.clone());
    }
    choice
        .listenables
        .iter()
        .find(|listenable| is_default_output(listenable))
        .cloned()
}

/// A device by its id, or a running app by its executable name, ignoring ASCII case as Windows
/// does. When several running apps share the name, such as two separate browser installs, the
/// first listed wins. Every listed app has an audio session.
pub fn find_remembered<'a>(
    listenables: &'a [Listenable],
    remembered: &RememberedSource,
) -> Option<&'a Listenable> {
    match remembered {
        RememberedSource::Device { source_id } => find(listenables, source_id),
        RememberedSource::App { name } => listenables.iter().find(|listenable| {
            matches!(
                listenable,
                Listenable::Capture(source)
                    if source.kind == SourceKind::App && source.name.eq_ignore_ascii_case(name)
            )
        }),
    }
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
