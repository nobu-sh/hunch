//! The overlay window's own commands: its remembered settings, staying on top, and moving and
//! sizing it in one step.

use std::sync::{Mutex, MutexGuard, PoisonError};

use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};

use crate::bounds::{self, Bounds};
use crate::cloak;
use crate::tray;
use crate::window_settings::{SavedPosition, WindowSettings, WindowSettingsFile};

/// Must match the window label in tauri.conf.json.
pub const MAIN_WINDOW: &str = "main";
/// Must match the event name in the UI. Its payload is whether the window stays on top.
const ALWAYS_ON_TOP_EVENT: &str = "always-on-top";

pub struct WindowState {
    file: WindowSettingsFile,
    settings: Mutex<WindowSettings>,
}

impl WindowState {
    pub fn load(file: WindowSettingsFile) -> Self {
        let settings = Mutex::new(file.load());
        Self { file, settings }
    }

    pub fn settings(&self) -> WindowSettings {
        self.lock().clone()
    }

    /// Changes the settings and saves them. A failed save is logged: the change still applies
    /// for this run.
    fn update(&self, change: impl FnOnce(&mut WindowSettings)) {
        let mut settings = self.lock();
        change(&mut settings);
        if let Err(error) = self.file.save(&settings) {
            eprintln!("hunch: couldn't save the window settings: {error}");
        }
    }

    fn lock(&self) -> MutexGuard<'_, WindowSettings> {
        self.settings.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[tauri::command]
pub fn window_settings(state: State<'_, WindowState>) -> WindowSettings {
    state.settings()
}

#[tauri::command]
pub fn save_window_position(state: State<'_, WindowState>, position: SavedPosition) {
    state.update(|settings| settings.position = Some(position));
}

#[tauri::command]
pub fn set_always_on_top(app: AppHandle, enabled: bool) -> Result<(), String> {
    apply_always_on_top(&app, enabled)
}

/// Hides the window from the screen across a resize, while its webview paints the new size.
#[tauri::command]
pub fn set_window_cloaked(window: WebviewWindow, cloaked: bool) -> Result<(), String> {
    cloak::set(&window, cloaked)
}

/// The header's close button: the window goes to the tray and keeps listening.
#[tauri::command]
pub fn hide_to_tray(app: AppHandle) {
    tray::set_visible(&app, false);
}

/// Moves and sizes the window together so its client area is at `bounds`, in physical pixels,
/// and a resize around a corner other than the top-left doesn't jump. A sync command, so it runs
/// on the main thread.
#[tauri::command]
pub fn set_window_bounds(window: WebviewWindow, bounds: Bounds) -> Result<(), String> {
    bounds::set_client(&window, bounds)
}

/// Applies, remembers and announces always-on-top, for the header toggle and the tray alike.
pub fn apply_always_on_top(app: &AppHandle, enabled: bool) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        window
            .set_always_on_top(enabled)
            .map_err(|error| format!("couldn't change always-on-top: {error}"))?;
    }
    app.state::<WindowState>()
        .update(|settings| settings.always_on_top = enabled);
    tray::show_always_on_top(app, enabled);
    if let Err(error) = app.emit(ALWAYS_ON_TOP_EVENT, enabled) {
        eprintln!("hunch: couldn't send {ALWAYS_ON_TOP_EVENT}: {error}");
    }
    Ok(())
}
