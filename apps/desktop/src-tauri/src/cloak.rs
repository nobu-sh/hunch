//! Hiding the window from the screen without hiding it from the app. The webview paints a new
//! size a frame or two after the window takes it, so the UI cloaks the window across a resize
//! instead of showing bare blur or half-drawn content.

use tauri::WebviewWindow;

/// Cloaks or uncloaks the window. A cloaked window keeps its size, place and focus, and its
/// webview keeps painting.
#[cfg(windows)]
pub fn set(window: &WebviewWindow, cloaked: bool) -> Result<(), String> {
    use std::ffi::c_void;

    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Dwm::{DWMWA_CLOAK, DwmSetWindowAttribute};
    use windows::core::BOOL;

    let handle = window
        .hwnd()
        .map_err(|error| format!("couldn't find the window handle: {error}"))?;
    // Tauri's HWND comes from an older `windows` crate; both wrap the same pointer.
    let hwnd = HWND(handle.0);
    let value = BOOL::from(cloaked);
    // SAFETY: the handle is a live window, and the attribute is a BOOL read from a local.
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_CLOAK,
            std::ptr::from_ref::<BOOL>(&value).cast::<c_void>(),
            size_of::<BOOL>() as u32,
        )
    }
    .map_err(|error| format!("couldn't cloak the window: {error}"))
}

/// Elsewhere a resize isn't hidden.
#[cfg(not(windows))]
pub fn set(_window: &WebviewWindow, _cloaked: bool) -> Result<(), String> {
    Ok(())
}
