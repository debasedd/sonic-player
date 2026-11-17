//! Sonic — Liquid Glass Spotify mini player.
//!
//! The Rust side owns every secret: the Spotify client id, PKCE verifier, access
//! and refresh tokens never reach the webview. The frontend talks to this module
//! through Tauri commands only.

pub mod auth;
pub mod clipboard;
#[cfg(windows)]
pub mod clipboard_sys;
#[cfg(windows)]
pub mod clipboard_watch;
pub mod commands;
pub mod config;
pub mod drag_drop;
pub mod error;
pub mod hover;
pub mod local_media;
pub mod player;
pub mod pocket;
pub mod shelf;
pub mod spotify;
pub mod state;
pub mod tray;
pub mod vault;
pub mod window;

use state::AppState;
use tauri::Manager;

/// Entry point used by `main.rs`.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let state = AppState::new();

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            // A second launch should surface the existing player, not spawn a
            // duplicate process.
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.show();
                let _ = win.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .manage(state.clone())
        .setup(|app| {
            let handle = app.handle().clone();

            // Center on the primary monitor before first paint, regardless of any
            // position saved by older builds.
            let main = window::create_main_window(&handle)?;

            // Native file drag-and-drop. Attached to the webview rather than the
            // webview's JS layer: Tauri replaces the WebView2 drop handler, so a
            // DOM `dragover`/`drop` pair never sees an Explorer drag. This hook
            // is the documented Rust-side entry point and is called on the
            // thread that owns the window.
            drag_drop::attach(main);

            // Acrylic is a native Win32 backdrop: CSS backdrop-filter cannot blur
            // the desktop behind a transparent Tauri window.
            window::apply_window_effects(&handle);

            tray::setup(&handle)?;

            // Basic local play/pause uses GSMTC and does not poll Spotify Web API.
            let media_state = handle.state::<AppState>().inner().clone();
            local_media::spawn_poller(handle.clone(), media_state);

            // Debounced position save. Without this, a crash or a force-kill lost
            // the position entirely, because the only other writer was the
            // shutdown hook.
            window::watch_geometry(handle.clone());

            // Pointer-hover collapse (pill ↔ mini). Parks itself when disabled.
            hover::spawn(handle.clone());

            // Clipboard Vault capture. Parks itself when capture is off or
            // Private Mode is on, so a disabled vault costs one cheap poll.
            clipboard_watch::spawn(handle.clone());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_bootstrap,
            commands::start_login,
            commands::cancel_login,
            commands::logout,
            commands::get_devices,
            commands::select_device,
            commands::playback_command,
            commands::seek,
            commands::set_volume,
            commands::set_shuffle,
            commands::set_repeat,
            commands::open_spotify,
            commands::get_settings,
            commands::update_settings,
            commands::set_window_mode,
            commands::set_hover_mini,
            commands::set_pointer_claimed,
            commands::set_drop_target,
            commands::set_pocket_open,
            commands::set_settings_open,
            commands::window_set_size,
            commands::window_get_frame,
            commands::window_set_frame,
            commands::set_always_on_top,
            commands::hide_window,
            commands::show_window,
            commands::quit_app,
            commands::get_saved_geometry,
            pocket::shelf_import,
            pocket::shelf_list,
            pocket::shelf_set_pinned,
            pocket::shelf_remove,
            pocket::shelf_clear,
            pocket::shelf_open,
            pocket::shelf_reveal,
            pocket::shelf_drag_out,
            pocket::clipboard_list,
            pocket::clipboard_restore,
            pocket::clipboard_set_pinned,
            pocket::clipboard_remove,
            pocket::clipboard_clear,
            pocket::clipboard_capture_now,
            pocket::clipboard_prune,
            pocket::pocket_usage,
        ]);

    let app = builder
        .build(tauri::generate_context!())
        .expect("error while building Sonic");

    app.run(|app_handle, event| {
        if let tauri::RunEvent::ExitRequested { .. } = event {
            // Persist geometry one last time on the way out.
            //
            // `AppState::persist_geometry` used to be a no-op with a comment
            // claiming window.rs did the writing. It does not: the shutdown hook
            // is the only thing that runs on a tray Quit, so a no-op here meant
            // the position was silently lost on the most common exit path.
            crate::window::persist_geometry(app_handle);
        }
    });
}
