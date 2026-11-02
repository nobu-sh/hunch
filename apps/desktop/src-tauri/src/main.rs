//! The Hunch desktop app: a `hunch-core` session in the Tauri backend, shown by a webview UI.

// Release builds have no console window on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod bounds;
mod capture_feed;
mod commands;
mod feed;
mod listener;
mod overlay_window;
mod remembered;
mod silence;
mod sources;
mod status;
mod test_beat;
mod tray;
mod window_settings;

use tauri::Manager;

use crate::commands::AppState;
use crate::listener::Listener;
use crate::overlay_window::{MAIN_WINDOW, WindowState};
use crate::remembered::RememberedSourceFile;
use crate::window_settings::WindowSettingsFile;

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            app.manage(AppState {
                backend: hunch_capture::default_backend(),
                listener: Listener::default(),
                remembered: RememberedSourceFile::in_dir(&data_dir),
            });
            // The UI places and shows the window; staying on top is set before it appears.
            let window_state = WindowState::load(WindowSettingsFile::in_dir(&data_dir));
            let always_on_top = window_state.settings().always_on_top;
            app.manage(window_state);
            if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
                window.set_always_on_top(always_on_top)?;
            }
            tray::create(app, always_on_top)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_sources,
            commands::select_source,
            commands::current_source,
            commands::listen,
            overlay_window::window_settings,
            overlay_window::save_window_position,
            overlay_window::set_always_on_top,
            overlay_window::set_window_bounds,
        ])
        .run(tauri::generate_context!())
        .expect("Hunch failed to start its window");
}
