//! The Hunch desktop app: a `hunch-core` session in the Tauri backend, shown by a webview UI.

// Release builds have no console window on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod feed;
mod listener;
mod test_beat;

use tauri::{AppHandle, State};

use crate::listener::Listener;
use crate::test_beat::TestBeatFeed;

/// Starts listening from scratch. The UI calls it once it's subscribed, so it sees every event.
#[tauri::command]
fn listen(app: AppHandle, listener: State<'_, Listener>) {
    listener.restart(listener::Restart {
        app,
        feed: Box::new(TestBeatFeed::new()),
    });
}

fn main() {
    tauri::Builder::default()
        .manage(Listener::default())
        .invoke_handler(tauri::generate_handler![listen])
        .run(tauri::generate_context!())
        .expect("Hunch failed to start its window");
}
