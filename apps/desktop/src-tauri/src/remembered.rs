//! The source the user last selected, kept as a small JSON file in the app data dir.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const FILE_NAME: &str = "remembered-source.json";

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RememberedSource {
    source_id: String,
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

    /// The saved id, or `None` when nothing was saved or the file can't be read.
    pub fn load(&self) -> Option<String> {
        let text = match fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(error) if error.kind() == ErrorKind::NotFound => return None,
            Err(error) => {
                eprintln!("hunch: couldn't read {}: {error}", self.path.display());
                return None;
            }
        };
        match serde_json::from_str::<RememberedSource>(&text) {
            Ok(remembered) => Some(remembered.source_id),
            Err(error) => {
                eprintln!("hunch: ignoring malformed {}: {error}", self.path.display());
                None
            }
        }
    }

    pub fn save(&self, source_id: &str) -> Result<(), String> {
        let remembered = RememberedSource {
            source_id: source_id.to_owned(),
        };
        let text = serde_json::to_string(&remembered).map_err(|error| error.to_string())?;
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
