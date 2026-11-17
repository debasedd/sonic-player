//! System tray icon and its context menu.
//!
//! The tray is the only way to reach Sonic when the window is hidden, so it
//! carries the transport controls as well as Show/Hide/Settings/Quit.

use crate::config::WindowMode;
use crate::player::EVENT_STATE;
use crate::spotify::SpotifyClient;
use crate::state::AppState;
use crate::window::MAIN_LABEL;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager, Runtime};

/// Builds the tray icon and menu, then stores it on the app state.
pub fn setup<R: Runtime>(app: &tauri::AppHandle<R>) -> tauri::Result<()> {
    let show_item = MenuItem::with_id(app, "show", "Show Sonic", true, None::<&str>)?;
    let hide_item = MenuItem::with_id(app, "hide", "Hide Sonic", true, None::<&str>)?;
    let play_pause_item = MenuItem::with_id(app, "playpause", "Play / Pause", true, None::<&str>)?;
    let next_item = MenuItem::with_id(app, "next", "Next Track", true, None::<&str>)?;
    let settings_item = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let quit_item = PredefinedMenuItem::quit(app, Some("Quit Sonic"))?;

    let menu = Menu::with_items(
        app,
        &[
            &show_item,
            &hide_item,
            &play_pause_item,
            &next_item,
            &settings_item,
            &quit_item,
        ],
    )?;

    let icon = app.default_window_icon().cloned().unwrap_or_else(|| {
        tauri::image::Image::new(&include_bytes!("../icons/tray.png")[..], 32, 32)
    });

    let builder = TrayIconBuilder::with_id("main")
        .icon(icon)
        .tooltip("Sonic")
        .menu(&menu)
        // Left-click toggles the window; right-click still opens the menu.
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => show_window(app),
            "hide" => hide_window(app),
            "playpause" => toggle_play_pause(app),
            "next" => next_track(app),
            "settings" => open_settings(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                toggle_window(app);
            }
        });

    let tray = builder.build(app)?;

    // Respect the "show tray icon" preference at startup.
    let state = app.state::<crate::state::AppState>();
    if !state.settings().show_tray {
        let _ = tray.set_visible(false);
    }

    app.manage(tray);
    Ok(())
}

/// The tray handle, if it exists.
fn tray_of<R: Runtime>(app: &tauri::AppHandle<R>) -> Option<tauri::tray::TrayIcon<R>> {
    app.try_state::<tauri::tray::TrayIcon<R>>()
        .map(|state| state.inner().clone())
}

/// Shows or hides the tray icon at runtime.
pub fn set_tray_visible<R: Runtime>(app: &tauri::AppHandle<R>, visible: bool) {
    if let Some(tray) = tray_of(app) {
        let _ = tray.set_visible(visible);
    }
}

/// Updates the tray tooltip to reflect what is playing.
pub fn set_tooltip<R: Runtime>(app: &tauri::AppHandle<R>, text: &str) {
    if let Some(tray) = tray_of(app) {
        let _ = tray.set_tooltip(Some(text));
    }
}

fn toggle_window<R: Runtime>(app: &tauri::AppHandle<R>) {
    let Some(window) = app.get_webview_window(MAIN_LABEL) else {
        return;
    };
    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
    } else {
        show_window(app);
    }
}

/// Shows and focuses the player.
pub fn show_window<R: Runtime>(app: &tauri::AppHandle<R>) {
    let Some(window) = app.get_webview_window(MAIN_LABEL) else {
        return;
    };
    let _ = window.show();
    let _ = window.set_focus();
}

/// Hides the player. The app keeps running so playback is unaffected.
pub fn hide_window<R: Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(window) = app.get_webview_window(MAIN_LABEL) {
        let _ = window.hide();
    }
}

/// Toggles the window's visibility.
pub fn toggle<R: Runtime>(app: &tauri::AppHandle<R>) {
    toggle_window(app);
}

/// Opens the settings view inside the player (it is a mode, not a new window).
pub fn open_settings<R: Runtime>(app: &tauri::AppHandle<R>) {
    push_state(app);

    if let Some(window) = app.get_webview_window(MAIN_LABEL) {
        let _ = window.show();
        let _ = window.set_focus();
        // The frontend switches to its settings panel on this event.
        let _ = app.emit("sonic://open-settings", ());
    }
}

fn toggle_play_pause<R: Runtime>(app: &tauri::AppHandle<R>) {
    let state = app.state::<AppState>();
    match crate::local_media::set_playing(None) {
        Ok(true) => {
            if let Ok(snapshot) = crate::local_media::snapshot() {
                state.set_local_media(snapshot);
            }
            push_state(app);
        }
        Ok(false) => {
            state.set_error(crate::error::AppError::Local(
                "No local media session. Open Spotify and start playback first.".into(),
            ));
            push_state(app);
        }
        Err(err) => {
            state.set_error(crate::error::AppError::Local(format!(
                "Could not control local Windows media session: {err}"
            )));
            push_state(app);
        }
    }
}

fn next_track<R: Runtime>(app: &tauri::AppHandle<R>) {
    let state = app.state::<AppState>();
    let Some(client) = state.ensure_client() else {
        return;
    };
    let device = state.settings().device_id.clone();
    match client.next(device.as_deref()) {
        Ok(()) => push_state(app),
        Err(err) => {
            state.set_error(err);
            push_state(app);
        }
    }
}

/// Convenience used by the command layer: push state after a change.
pub fn push_state<R: Runtime>(app: &tauri::AppHandle<R>) {
    let state = app.state::<AppState>();
    let snapshot = state.ui_state();
    if let Err(err) = app.emit(EVENT_STATE, &snapshot) {
        eprintln!("Sonic: emit failed: {err}");
    }
}

/// Applies the window mode and updates the tray tooltip.
pub fn apply_mode<R: Runtime>(app: &tauri::AppHandle<R>, mode: WindowMode) {
    crate::window::apply_mode(app, mode);
}

/// Builds a client guard used by commands that need a client.
pub fn with_client<R, T, F>(app: &tauri::AppHandle<R>, f: F) -> Option<T>
where
    R: Runtime,
    F: FnOnce(&SpotifyClient) -> T,
{
    let state = app.state::<AppState>();
    state.ensure_client().map(|client| f(&client))
}
