//! Moving and sizing a window at once. Tauri only moves and sizes in separate steps, and a frame
//! can show between them.

use serde::Deserialize;
use tauri::WebviewWindow;

/// A rectangle in physical screen pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct Bounds {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// How far a window's outer edges sit outside its client area. An undecorated window on Windows
/// keeps invisible resize borders there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Insets {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

/// The outer rectangle that puts the client area at `client`.
pub fn outer_bounds(client: Bounds, insets: Insets) -> Bounds {
    Bounds {
        x: client.x - insets.left,
        y: client.y - insets.top,
        width: client.width + insets.left + insets.right,
        height: client.height + insets.top + insets.bottom,
    }
}

/// Moves and sizes the window so its client area, the part the page draws, is at `client`.
#[cfg(windows)]
pub fn set_client(window: &WebviewWindow, client: Bounds) -> Result<(), String> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{SWP_NOACTIVATE, SWP_NOZORDER, SetWindowPos};

    let handle = window
        .hwnd()
        .map_err(|error| format!("couldn't find the window handle: {error}"))?;
    // Tauri's HWND comes from an older `windows` crate; both wrap the same pointer.
    let hwnd = HWND(handle.0);
    let outer = outer_bounds(client, insets(hwnd)?);
    // SAFETY: the handle belongs to a live Tauri window, and commands that call this run on its
    // UI thread.
    unsafe {
        SetWindowPos(
            hwnd,
            None,
            outer.x,
            outer.y,
            outer.width,
            outer.height,
            SWP_NOZORDER | SWP_NOACTIVATE,
        )
    }
    .map_err(|error| format!("couldn't move the window to {outer:?}: {error}"))
}

#[cfg(windows)]
fn insets(hwnd: windows::Win32::Foundation::HWND) -> Result<Insets, String> {
    use windows::Win32::Foundation::{POINT, RECT};
    use windows::Win32::Graphics::Gdi::ClientToScreen;
    use windows::Win32::UI::WindowsAndMessaging::{GetClientRect, GetWindowRect};

    let couldnt = |error: windows::core::Error| format!("couldn't measure the window: {error}");
    let mut outer = RECT::default();
    let mut client = RECT::default();
    let mut origin = POINT::default();
    // SAFETY: the handle is a live window and every pointer is to a local.
    unsafe {
        GetWindowRect(hwnd, &mut outer).map_err(couldnt)?;
        GetClientRect(hwnd, &mut client).map_err(couldnt)?;
        ClientToScreen(hwnd, &mut origin).ok().map_err(couldnt)?;
    }
    Ok(Insets {
        left: origin.x - outer.left,
        top: origin.y - outer.top,
        right: outer.right - (origin.x + client.right),
        bottom: outer.bottom - (origin.y + client.bottom),
    })
}

/// Elsewhere Tauri's position is the outer one and its size the inner one, so the insets are
/// left at zero.
#[cfg(not(windows))]
pub fn set_client(window: &WebviewWindow, client: Bounds) -> Result<(), String> {
    use tauri::{PhysicalPosition, PhysicalSize};

    let couldnt = |error: tauri::Error| format!("couldn't move the window to {client:?}: {error}");
    let outer = outer_bounds(client, Insets::default());
    let to_size = |length: i32| {
        u32::try_from(length).map_err(|error| format!("bad window size in {client:?}: {error}"))
    };
    let size = PhysicalSize::new(to_size(outer.width)?, to_size(outer.height)?);
    window
        .set_position(PhysicalPosition::new(outer.x, outer.y))
        .map_err(couldnt)?;
    window.set_size(size).map_err(couldnt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outer_bounds_wrap_the_client_in_the_insets() {
        let client = Bounds {
            x: 2244,
            y: 16,
            width: 300,
            height: 171,
        };
        let insets = Insets {
            left: 8,
            top: 1,
            right: 8,
            bottom: 8,
        };

        let outer = Bounds {
            x: 2236,
            y: 15,
            width: 316,
            height: 180,
        };
        assert_eq!(outer_bounds(client, insets), outer);
    }

    #[test]
    fn no_insets_keep_the_client_rect() {
        let client = Bounds {
            x: -10,
            y: 5,
            width: 120,
            height: 32,
        };

        assert_eq!(outer_bounds(client, Insets::default()), client);
    }
}
