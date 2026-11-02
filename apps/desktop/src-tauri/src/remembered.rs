//! The source the user last selected, kept as a small JSON file in the app data dir.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use hunch_capture::SourceKind;
use serde::{Deserialize, Serialize};

use crate::sources::Listenable;

const FILE_NAME: &str = "remembered-source.json";

/// Enough to find the selected source again in a later run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum RememberedSource {
    /// A device or the dev source, by id: those ids stay the same across runs.
    Device { source_id: String },
    /// An app, by executable name: its id holds a process id, which changes when it restarts.
    App { name: String },
}

impl RememberedSource {
    pub fn of(listenable: &Listenable) -> Self {
        match listenable {
            Listenable::Capture(source) if source.kind == SourceKind::App => Self::App {
                name: source.name.clone(),
            },
            _ => Self::Device {
                source_id: listenable.id().to_owned(),
            },
        }
    }
}

/// The file as written by any version of the app.
#[derive(Deserialize)]
#[serde(untagged)]
enum StoredSource {
    Current(RememberedSource),
    /// Before apps were remembered by name, only the id was saved. Matching it as an id keeps
    /// devices working; an old app id names a process that has likely exited.
    #[serde(rename_all = "camelCase")]
    IdOnly {
        source_id: String,
    },
}

impl From<StoredSource> for RememberedSource {
    fn from(stored: StoredSource) -> Self {
        match stored {
            StoredSource::Current(remembered) => remembered,
            StoredSource::IdOnly { source_id } => Self::Device { source_id },
        }
    }
}

pub struct RememberedSourceFile {
    path: PathBuf,
}

impl RememberedSourceFile {
    pub fn in_dir(dir: &Path) -> Self {
        Self {
            path: dir.join(FILE_NAME),
        }
    }

    /// The saved source, or `None` when nothing was saved or the file can't be read.
    pub fn load(&self) -> Option<RememberedSource> {
        let text = match fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(error) if error.kind() == ErrorKind::NotFound => return None,
            Err(error) => {
                eprintln!("hunch: couldn't read {}: {error}", self.path.display());
                return None;
            }
        };
        match serde_json::from_str::<StoredSource>(&text) {
            Ok(stored) => Some(stored.into()),
            Err(error) => {
                eprintln!("hunch: ignoring malformed {}: {error}", self.path.display());
                None
            }
        }
    }

    pub fn save(&self, remembered: &RememberedSource) -> Result<(), String> {
        let text = serde_json::to_string(remembered).map_err(|error| error.to_string())?;
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)
                .map_err(|error| format!("couldn't create {}: {error}", dir.display()))?;
        }
        fs::write(&self.path, text)
            .map_err(|error| format!("couldn't write {}: {error}", self.path.display()))
    }
}

#[cfg(test)]
mod tests;
