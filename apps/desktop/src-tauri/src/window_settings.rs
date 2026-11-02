//! How the window was left: where it sits and whether it stays on top, as a small JSON file in
//! the app data dir.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const FILE_NAME: &str = "window-settings.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

/// Where the window sits: logical offsets from a corner of a monitor's work area, so it stays
/// put when the resolution, taskbar or monitor layout changes. The UI does the placing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedPosition {
    /// The monitor's name. `None`, or a monitor that's gone, means the primary monitor.
    pub monitor: Option<String>,
    pub corner: Corner,
    pub offset_x: f64,
    pub offset_y: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct WindowSettings {
    /// `None` until the window is first dragged; the UI then uses its default corner.
    pub position: Option<SavedPosition>,
    pub always_on_top: bool,
}

impl Default for WindowSettings {
    fn default() -> Self {
        // On top by default: the point is to float over a DAW or a browser.
        Self {
            position: None,
            always_on_top: true,
        }
    }
}

pub struct WindowSettingsFile {
    path: PathBuf,
}

impl WindowSettingsFile {
    pub fn in_dir(dir: &Path) -> Self {
        Self {
            path: dir.join(FILE_NAME),
        }
    }

    /// The saved settings, with defaults for anything missing, unreadable or malformed.
    pub fn load(&self) -> WindowSettings {
        let text = match fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(error) if error.kind() == ErrorKind::NotFound => return WindowSettings::default(),
            Err(error) => {
                eprintln!("hunch: couldn't read {}: {error}", self.path.display());
                return WindowSettings::default();
            }
        };
        serde_json::from_str(&text).unwrap_or_else(|error| {
            eprintln!("hunch: ignoring malformed {}: {error}", self.path.display());
            WindowSettings::default()
        })
    }

    pub fn save(&self, settings: &WindowSettings) -> Result<(), String> {
        let text = serde_json::to_string(settings).map_err(|error| error.to_string())?;
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
