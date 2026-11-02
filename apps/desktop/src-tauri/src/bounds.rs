//! Moving and sizing a window at once. Tauri only moves and sizes in separate steps, and a frame
//! can show between them.

use tauri::WebviewWindow;

/// A window's outer rectangle in physical pixels.
#[derive(Debug, Clone, Copy)]
pub struct Bounds {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[cfg(windows)]
pub fn set(window: &WebviewWindow, bounds: Bounds) -> Result<(), String> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{SWP_NOACTIVATE, SWP_NOZORDER, SetWindowPos};

    let handle = window
        .hwnd()
        .map_err(|error| format!("couldn't find the window handle: {error}"))?;
    // Tauri's HWND comes from an older `windows` crate; both wrap the same pointer.
    let hwnd = HWND(handle.0);
    let width = i32::try_from(bounds.width).map_err(|error| error.to_string())?;
    let height = i32::try_from(bounds.height).map_err(|error| error.to_string())?;
    // SAFETY: the handle belongs to a live Tauri window, and commands that call this run on its
    // UI thread.
    unsafe {
        SetWindowPos(
            hwnd,
            None,
            bounds.x,
            bounds.y,
            width,
            height,
            SWP_NOZORDER | SWP_NOACTIVATE,
        )
    }
    .map_err(|error| format!("couldn't move the window to {bounds:?}: {error}"))
}

#[cfg(not(windows))]
pub fn set(window: &WebviewWindow, bounds: Bounds) -> Result<(), String> {
    use tauri::{PhysicalPosition, PhysicalSize};

    let couldnt = |error: tauri::Error| format!("couldn't move the window to {bounds:?}: {error}");
    window
        .set_position(PhysicalPosition::new(bounds.x, bounds.y))
        .map_err(couldnt)?;
    window
        .set_size(PhysicalSize::new(bounds.width, bounds.height))
        .map_err(couldnt)
}
