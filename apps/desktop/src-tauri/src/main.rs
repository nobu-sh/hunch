//! The Hunch desktop app: a `hunch-core` session in the Tauri backend, shown by a webview UI.

// Release builds have no console window on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod capture_feed;
mod commands;
mod feed;
mod listener;
mod remembered;
mod silence;
mod sources;
mod status;
mod test_beat;

use tauri::Manager;

use crate::commands::AppState;
use crate::listener::Listener;
use crate::remembered::RememberedSourceFile;

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            app.manage(AppState {
                backend: hunch_capture::default_backend(),
                listener: Listener::default(),
                remembered: RememberedSourceFile::in_dir(&data_dir),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_sources,
            commands::select_source,
            commands::current_source,
            commands::listen,
        ])
        .run(tauri::generate_context!())
        .expect("Hunch failed to start its window");
}
