use std::process;
use std::sync::atomic::{AtomicU32, Ordering};

use super::*;

static NEXT_DIR: AtomicU32 = AtomicU32::new(0);

/// A fresh directory per test, removed when dropped.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let name = format!(
            "hunch-window-settings-{}-{}",
            process::id(),
            NEXT_DIR.fetch_add(1, Ordering::Relaxed)
        );
        Self(std::env::temp_dir().join(name))
    }

    fn write(&self, text: &str) -> WindowSettingsFile {
        fs::create_dir_all(&self.0).unwrap();
        fs::write(self.0.join(FILE_NAME), text).unwrap();
        WindowSettingsFile::in_dir(&self.0)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn bottom_right() -> SavedPosition {
    SavedPosition {
        monitor: Some(r"\.\DISPLAY2".to_owned()),
        corner: Corner::BottomRight,
        offset_x: 24.0,
        offset_y: 12.5,
    }
}

#[test]
fn nothing_saved_loads_the_defaults() {
    let dir = TempDir::new();

    let settings = WindowSettingsFile::in_dir(&dir.0).load();

    assert_eq!(settings, WindowSettings::default());
    assert!(settings.always_on_top);
    assert_eq!(settings.position, None);
}

#[test]
fn saved_settings_load_back() {
    let dir = TempDir::new();
    let file = WindowSettingsFile::in_dir(&dir.0.join("nested"));
    let settings = WindowSettings {
        position: Some(bottom_right()),
        always_on_top: false,
    };

    file.save(&settings).unwrap();

    assert_eq!(file.load(), settings);
}

#[test]
fn malformed_file_loads_the_defaults() {
    let dir = TempDir::new();

    let file = dir.write("{ not json");

    assert_eq!(file.load(), WindowSettings::default());
}

#[test]
fn unknown_corner_loads_the_defaults() {
    let dir = TempDir::new();

    let file = dir.write(
        r#"{"position":{"monitor":null,"corner":"middle","offsetX":0,"offsetY":0},"alwaysOnTop":false}"#,
    );

    assert_eq!(file.load(), WindowSettings::default());
}

#[test]
fn missing_fields_take_their_defaults() {
    let dir = TempDir::new();

    let file = dir.write(r#"{"position":{"corner":"top-left","offsetX":5,"offsetY":6}}"#);

    let expected = WindowSettings {
        position: Some(SavedPosition {
            monitor: None,
            corner: Corner::TopLeft,
            offset_x: 5.0,
            offset_y: 6.0,
        }),
        always_on_top: true,
    };
    assert_eq!(file.load(), expected);
}

#[test]
fn json_matches_the_ui() {
    let settings = WindowSettings {
        position: Some(bottom_right()),
        always_on_top: true,
    };

    let json = serde_json::to_value(&settings).unwrap();

    let expected = serde_json::json!({
        "position": {
            "monitor": r"\.\DISPLAY2",
            "corner": "bottom-right",
            "offsetX": 24.0,
            "offsetY": 12.5,
        },
        "alwaysOnTop": true,
    });
    assert_eq!(json, expected);
}
