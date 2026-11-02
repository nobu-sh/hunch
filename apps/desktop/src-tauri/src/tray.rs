//! The tray icon: a left click shows or hides the window, and its menu shows or hides it,
//! toggles always-on-top and quits.

use tauri::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{App, AppHandle, Manager, Wry};

use crate::overlay_window::{self, MAIN_WINDOW, WindowState};

const TOGGLE_VISIBILITY_ID: &str = "toggle-visibility";
const ALWAYS_ON_TOP_ID: &str = "always-on-top";
const QUIT_ID: &str = "quit";
const HIDE_TEXT: &str = "Hide Hunch";
const SHOW_TEXT: &str = "Show Hunch";

/// The menu items whose text or check mark follows the window.
struct TrayItems {
    toggle_visibility: MenuItem<Wry>,
    always_on_top: CheckMenuItem<Wry>,
}

pub fn create(app: &App, always_on_top: bool) -> tauri::Result<()> {
    let toggle_visibility =
        MenuItem::with_id(app, TOGGLE_VISIBILITY_ID, HIDE_TEXT, true, None::<&str>)?;
    let always_on_top = CheckMenuItem::with_id(
        app,
        ALWAYS_ON_TOP_ID,
        "Always on top",
        true,
        always_on_top,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, QUIT_ID, "Quit", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(
        app,
        &[&toggle_visibility, &always_on_top, &separator, &quit],
    )?;

    let mut tray = TrayIconBuilder::new()
        .tooltip("Hunch")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(on_menu_event)
        .on_tray_icon_event(on_tray_icon_event);
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    app.manage(TrayItems {
        toggle_visibility,
        always_on_top,
    });
    Ok(())
}

/// Keeps the menu's check mark in step with the window.
pub fn show_always_on_top(app: &AppHandle, enabled: bool) {
    let Some(items) = app.try_state::<TrayItems>() else {
        return;
    };
    if let Err(error) = items.always_on_top.set_checked(enabled) {
        eprintln!("hunch: couldn't update the tray menu: {error}");
    }
}

fn on_menu_event(app: &AppHandle, event: MenuEvent) {
    match event.id().as_ref() {
        TOGGLE_VISIBILITY_ID => toggle_visibility(app),
        ALWAYS_ON_TOP_ID => toggle_always_on_top(app),
        QUIT_ID => app.exit(0),
        _ => {}
    }
}

fn on_tray_icon_event(tray: &TrayIcon, event: TrayIconEvent) {
    let TrayIconEvent::Click {
        button: MouseButton::Left,
        button_state: MouseButtonState::Up,
        ..
    } = event
    else {
        return;
    };
    toggle_visibility(tray.app_handle());
}

/// The menu flips its own check mark when clicked, so the saved setting decides the new state.
fn toggle_always_on_top(app: &AppHandle) {
    let enabled = app.state::<WindowState>().settings().always_on_top;
    if let Err(error) = overlay_window::apply_always_on_top(app, !enabled) {
        eprintln!("hunch: {error}");
    }
}

fn toggle_visibility(app: &AppHandle) {
    let Some(window) = app.get_webview_window(MAIN_WINDOW) else {
        return;
    };
    let is_visible = window.is_visible().unwrap_or(false);
    set_visible(app, !is_visible);
}

/// Shows or hides the window and names the menu item for the opposite. A hidden window keeps
/// its session running; only Quit exits.
pub fn set_visible(app: &AppHandle, visible: bool) {
    let Some(window) = app.get_webview_window(MAIN_WINDOW) else {
        return;
    };
    let result = if visible {
        window.show().and_then(|()| window.set_focus())
    } else {
        window.hide()
    };
    if let Err(error) = result {
        eprintln!("hunch: couldn't show or hide the window: {error}");
        return;
    }
    let text = if visible { HIDE_TEXT } else { SHOW_TEXT };
    let Some(items) = app.try_state::<TrayItems>() else {
        return;
    };
    if let Err(error) = items.toggle_visibility.set_text(text) {
        eprintln!("hunch: couldn't update the tray menu: {error}");
    }
}
