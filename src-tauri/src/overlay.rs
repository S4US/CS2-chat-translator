use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

pub const LABEL: &str = "overlay";

/// Shows, hides or locks the in-game overlay. A locked overlay lets mouse
/// clicks pass through to the game underneath.
pub fn set(app: &AppHandle, visible: bool, locked: bool) -> tauri::Result<()> {
    let existing = app.get_webview_window(LABEL);
    if !visible {
        if let Some(window) = existing {
            window.close()?;
        }
        return Ok(());
    }

    let window = match existing {
        Some(window) => window,
        None => WebviewWindowBuilder::new(app, LABEL, WebviewUrl::default())
            .title("CS2 Translator Overlay")
            .inner_size(480.0, 260.0)
            .min_inner_size(260.0, 120.0)
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .always_on_top(true)
            .visible_on_all_workspaces(true)
            .skip_taskbar(true)
            .focused(false)
            .build()?,
    };
    window.set_ignore_cursor_events(locked)
}
