//! The `source-status` event: how capture from the current source is going.

use serde::Serialize;

use crate::sources::Listenable;

/// The Tauri event that carries each `SourceStatusDto` to the webview.
pub const SOURCE_STATUS_EVENT: &str = "source-status";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceStatusDto {
    /// `None` when no source could be chosen at all.
    pub source_id: Option<String>,
    pub state: SourceState,
    /// Plain words for people. `None` while listening.
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SourceState {
    Listening,
    Ended,
    Silent,
    Failed,
}

impl SourceStatusDto {
    pub fn listening(source: &Listenable) -> Self {
        Self {
            source_id: Some(source.id().to_owned()),
            state: SourceState::Listening,
            message: None,
        }
    }

    pub fn ended(source: &Listenable) -> Self {
        let name = source.name();
        let message = if source.is_app() {
            format!("{name} closed, so there's nothing left to hear.")
        } else {
            format!("{name} was unplugged or turned off.")
        };
        Self {
            source_id: Some(source.id().to_owned()),
            state: SourceState::Ended,
            message: Some(message),
        }
    }

    pub fn silent(source: &Listenable) -> Self {
        Self {
            source_id: Some(source.id().to_owned()),
            state: SourceState::Silent,
            message: Some(format!(
                "{} is sending only silence. Nothing may be playing, or the audio may be \
                 protected (DRM).",
                source.name()
            )),
        }
    }

    pub fn failed(failure: Failure<'_>) -> Self {
        Self {
            source_id: failure.source.map(|source| source.id().to_owned()),
            state: SourceState::Failed,
            message: Some(failure.message.to_owned()),
        }
    }
}

pub struct Failure<'a> {
    pub source: Option<&'a Listenable>,
    pub message: &'a str,
}

#[cfg(test)]
mod tests;
