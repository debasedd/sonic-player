//! Tauri command handlers — the whole frontend API surface.
//!
//! Every command is synchronous from the frontend's point of view but does no
//! blocking work on the UI thread beyond a single HTTP call; Tauri runs each
//! command on its own worker.

use crate::config::{self, Geometry, WindowMode};
use crate::error::{AppError, AppResult};
use crate::player::{self, EVENT_STATE};
use crate::spotify::Device;
use crate::state::{UiSettings, UiState};
use crate::tray;
use crate::window::MAIN_LABEL;
use tauri::{AppHandle, Emitter, Manager};

/// A window frame in logical pixels.
///
/// Used by the frontend to tween between mode geometries. Logical, not physical:
/// Windows applies the DPI scale, and the frontend works in logical units so the
/// numbers it animates match the CSS it is drawing.
#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Frame {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Everything the frontend needs on first paint.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bootstrap {
    pub state: UiState,
    /// Whether the build has a usable client id (from env or saved settings).
    pub client_id_present: bool,
    /// The exact redirect URI to allow-list in the Spotify dashboard.
    pub redirect_uri: String,
    /// Platform string, so the UI can avoid offering impossible actions.
    pub platform: String,
    /// Whether native autostart is supported on this build.
    pub autostart_supported: bool,
}

/// Pushes a state snapshot to the frontend.
fn emit_state<R: tauri::Runtime>(app: &tauri::AppHandle<R>, state: &UiState) {
    if let Err(err) = app.emit(EVENT_STATE, state) {
        eprintln!("Sonic: emit failed: {err}");
    }
}

/// Pushes a state snapshot to the frontend.
///
/// Public so the Pocket commands can push through the same single path instead of
/// building their own emitter. One emitter means one event name, one payload
/// shape, and one place to fix if the snapshot contract ever changes.
pub fn push_state<R: tauri::Runtime>(app: &tauri::AppHandle<R>, state: &crate::state::AppState) {
    let snapshot = state.ui_state();
    emit_state(app, &snapshot);
}

/// Kept as a private alias so the ~20 existing call sites read the same.
fn push<R: tauri::Runtime>(app: &tauri::AppHandle<R>, state: &crate::state::AppState) {
    push_state(app, state);
}

/// Returns everything the UI needs at startup.
#[tauri::command]
pub fn get_bootstrap(app: AppHandle) -> Bootstrap {
    let state = app.state::<crate::state::AppState>();
    if let Ok(local_media) = crate::local_media::snapshot() {
        state.set_local_media(local_media);
    }
    let settings = state.settings();
    let ui = state.ui_state();

    // Adopt persisted identity into the snapshot if the poller has not run yet.
    let (stored_name, stored_avatar, stored_premium) = state.account();
    let account_name = if ui.account_name.is_empty() {
        stored_name
    } else {
        ui.account_name.clone()
    };
    let avatar_url = if ui.avatar_url.is_empty() {
        stored_avatar
    } else {
        ui.avatar_url.clone()
    };
    let is_premium = ui.is_premium || stored_premium;
    let account_label = if account_name.is_empty() {
        String::new()
    } else if is_premium {
        format!("{account_name} · Premium")
    } else {
        format!("{account_name} · Free")
    };

    let ui = UiState {
        account_name,
        avatar_url,
        is_premium,
        account_label,
        ..ui
    };

    Bootstrap {
        client_id_present: !settings.client_id.trim().is_empty(),
        redirect_uri: config::redirect_uri(settings.redirect_port),
        platform: std::env::consts::OS.to_string(),
        autostart_supported: config::autostart_supported(),
        state: ui,
    }
}

/// Begins an interactive Spotify sign-in.
///
/// Returns immediately; progress arrives as state events on the `sonic://state`
/// channel, with `authenticating` set while the browser round-trip is open.
#[tauri::command]
pub fn start_login(app: AppHandle) -> AppResult<()> {
    // Own the state handle: the worker thread below outlives this borrow of `app`.
    let state = app.state::<crate::state::AppState>().inner().clone();
    let settings = state.settings();

    if settings.client_id.trim().is_empty() {
        return Err(AppError::Local(
            "Add your Spotify client id in Settings before connecting.".into(),
        ));
    }
    if state.is_authenticating() {
        return Err(AppError::Local(
            "A sign-in is already in progress. Finish or cancel it first.".into(),
        ));
    }

    let (cancel_tx, cancel_rx) = std::sync::mpsc::channel::<()>();
    state.set_pending_login(Some(cancel_tx));
    state.set_authenticating(true);
    push(&app, &state);

    let login_state = state.clone();
    let login_settings = settings.clone();
    // Clone the handle: the worker thread outlives this command's borrow of `app`.
    let worker_app = app.clone();

    std::thread::spawn(move || {
        // `perform_login` blocks until the browser redirect arrives, the user
        // cancels, or the 5 minute deadline passes.
        match crate::auth::perform_login(&login_settings, &cancel_rx) {
            Ok(outcome) => {
                if login_state.install_tokens(outcome.tokens).is_ok() {
                    // Refresh the profile immediately so the label is right.
                    if let Some(client) = login_state.client() {
                        player::refresh_account(&client, &login_state);
                    }
                }
            }
            Err(err) => login_state.set_error(err),
        }

        login_state.set_authenticating(false);
        login_state.set_pending_login(None);
        let snapshot = login_state.ui_state();
        emit_state(&worker_app, &snapshot);
    });

    Ok(())
}

/// Cancels an in-flight sign-in.
#[tauri::command]
pub fn cancel_login(app: AppHandle) -> AppResult<()> {
    let state = app.state::<crate::state::AppState>();
    state.cancel_pending_login();
    push(&app, &state);
    Ok(())
}

/// Removes stored credentials.
#[tauri::command]
pub fn logout(app: AppHandle) -> AppResult<()> {
    let state = app.state::<crate::state::AppState>();
    state.cancel_pending_login();
    state.disconnect()?;
    push(&app, &state);
    Ok(())
}

/// Lists available playback devices.
#[tauri::command]
pub fn get_devices(app: AppHandle) -> AppResult<Vec<Device>> {
    let state = app.state::<crate::state::AppState>();
    let client = state.ensure_client().ok_or(AppError::Unauthorized)?;

    let devices = client.devices()?;
    state.set_devices(devices.clone());
    push(&app, &state);
    Ok(devices)
}

/// Moves playback to a specific device.
#[tauri::command]
pub fn select_device(app: AppHandle, device_id: String, play: Option<bool>) -> AppResult<()> {
    let state = app.state::<crate::state::AppState>();
    let client = state.ensure_client().ok_or(AppError::Unauthorized)?;

    client.transfer(&device_id, play.unwrap_or(true))?;

    // Remember the choice so the next command targets the same device.
    let mut settings = state.settings();
    settings.device_id = Some(device_id);
    state.update_settings(|s| *s = settings);

    // Pull a fresh snapshot so the UI shows the new device immediately.
    if let Ok(Some(playback)) = client.playback() {
        state.set_playback(Some(playback));
    }
    if let Ok(devices) = client.devices() {
        state.set_devices(devices);
    }
    push(&app, &state);
    Ok(())
}

/// Transport actions the UI can trigger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Transport {
    Play,
    Pause,
    Toggle,
    Next,
    Previous,
}

#[tauri::command]
pub fn playback_command(
    app: AppHandle,
    command: Transport,
    device_id: Option<String>,
) -> AppResult<()> {
    let state = app.state::<crate::state::AppState>();
    let local_available = state.local_media().available;

    // Every transport action is local-first: GSMTC needs no Spotify account.
    let local_action = match command {
        Transport::Play | Transport::Pause | Transport::Toggle => None,
        Transport::Next => Some("next"),
        Transport::Previous => Some("previous"),
    };

    if let Some(action) = local_action {
        match crate::local_media::local_command(action, None) {
            Ok(true) => {
                if let Ok(snapshot) = crate::local_media::snapshot() {
                    state.set_local_media(snapshot);
                }
                push(&app, &state);
                return Ok(());
            }
            Ok(false) if local_available => {
                return Err(AppError::Local(
                    "This media app does not support that control locally.".into(),
                ));
            }
            Ok(false) => {}
            Err(err) if local_available => {
                return Err(AppError::Local(format!(
                    "Could not control local Windows media session: {err}"
                )));
            }
            Err(_) => {}
        }
    } else {
        let desired = match command {
            Transport::Play => Some(true),
            Transport::Pause => Some(false),
            Transport::Toggle => None,
            Transport::Next | Transport::Previous => unreachable!(),
        };
        let toggled = crate::local_media::set_playing(desired).map_err(|err| {
            AppError::Local(format!(
                "Could not control local Windows media session: {err}"
            ))
        })?;
        if !toggled && local_available {
            return Err(AppError::Local(
                "This media app does not accept play/pause locally.".into(),
            ));
        }
        if toggled || local_available {
            if let Ok(snapshot) = crate::local_media::snapshot() {
                state.set_local_media(snapshot);
            }
            push(&app, &state);
            return Ok(());
        }
    }

    // Fallback: Spotify Connect, only when there is no usable local session.
    let client = state.ensure_client().ok_or_else(|| {
        AppError::Local(
            "No local media session and Spotify is not connected. Start playback in any app, or connect Spotify.".into(),
        )
    })?;
    let device = device_id.or_else(|| state.settings().device_id.clone());
    match command {
        Transport::Next => client.next(device.as_deref())?,
        Transport::Previous => client.previous(device.as_deref())?,
        Transport::Play => client.play(device.as_deref(), None)?,
        Transport::Pause => client.pause(device.as_deref())?,
        Transport::Toggle => {
            let playing = client
                .playback()
                .ok()
                .flatten()
                .map(|p| p.is_playing)
                .unwrap_or(false);
            if playing {
                client.pause(device.as_deref())?;
            } else {
                client.play(device.as_deref(), None)?;
            }
        }
    }

    if let Ok(Some(playback)) = client.playback() {
        state.set_playback(Some(playback));
    }
    push(&app, &state);
    Ok(())
}

/// Seeks within the current track, locally first and Spotify second.
#[tauri::command]
pub fn seek(app: AppHandle, position_ms: u64, device_id: Option<String>) -> AppResult<()> {
    let state = app.state::<crate::state::AppState>();

    match crate::local_media::local_command("seek", Some(&position_ms.to_string())) {
        Ok(true) => {
            if let Ok(snapshot) = crate::local_media::snapshot() {
                state.set_local_media(snapshot);
            }
            push(&app, &state);
            return Ok(());
        }
        Ok(false) if state.local_media().available => {
            return Err(AppError::Local(
                "This media app does not support seeking locally.".into(),
            ));
        }
        _ => {}
    }

    let client = state.ensure_client().ok_or_else(|| {
        AppError::Local("No local media session and Spotify is not connected.".into())
    })?;
    let device = device_id.or_else(|| state.settings().device_id.clone());
    client.seek(position_ms, device.as_deref())?;

    if let Some(mut playback) = state.cache().playback {
        playback.progress_ms = position_ms;
        state.set_playback(Some(playback));
    }
    push(&app, &state);
    Ok(())
}

/// Sets device volume, 0-100.
#[tauri::command]
pub fn set_volume(app: AppHandle, volume_percent: u8, device_id: Option<String>) -> AppResult<()> {
    let state = app.state::<crate::state::AppState>();
    let client = state.ensure_client().ok_or(AppError::Unauthorized)?;

    let volume = volume_percent.min(100);
    let device = device_id.or_else(|| state.settings().device_id.clone());
    client.volume(volume, device.as_deref())?;

    // Persist so the slider is where the user left it next launch.
    let mut settings = state.settings();
    settings.volume = volume;
    state.update_settings(|s| *s = settings);

    if let Some(mut playback) = state.cache().playback {
        playback.device.volume_percent = volume;
        state.set_playback(Some(playback));
    }
    push(&app, &state);
    Ok(())
}

/// Toggles shuffle: local session first, Spotify Connect as fallback.
#[tauri::command]
pub fn set_shuffle(app: AppHandle, state_value: bool, device_id: Option<String>) -> AppResult<()> {
    let state = app.state::<crate::state::AppState>();
    let local_available = state.local_media().available;

    match crate::local_media::local_command(
        "shuffle",
        Some(if state_value { "true" } else { "false" }),
    ) {
        Ok(true) => {
            if let Ok(snapshot) = crate::local_media::snapshot() {
                state.set_local_media(snapshot);
            }
            push(&app, &state);
            return Ok(());
        }
        Ok(false) if local_available => {
            return Err(AppError::Local(
                "This media app does not support shuffle locally.".into(),
            ));
        }
        _ => {}
    }

    let client = state.ensure_client().ok_or_else(|| {
        AppError::Local("No local media session and Spotify is not connected.".into())
    })?;
    let device = device_id.or_else(|| state.settings().device_id.clone());
    client.shuffle(state_value, device.as_deref())?;

    if let Some(mut playback) = state.cache().playback {
        playback.shuffle_state = state_value;
        state.set_playback(Some(playback));
    }
    push(&app, &state);
    Ok(())
}

/// Sets repeat mode: `off`, `track`, or `context`. Local first, Spotify second.
#[tauri::command]
pub fn set_repeat(
    app: AppHandle,
    repeat_state: String,
    device_id: Option<String>,
) -> AppResult<()> {
    let state = app.state::<crate::state::AppState>();
    let local_available = state.local_media().available;

    if !matches!(repeat_state.as_str(), "off" | "track" | "context") {
        return Err(AppError::Local(format!(
            "Unknown repeat mode: {repeat_state}"
        )));
    }

    match crate::local_media::local_command("repeat", Some(&repeat_state)) {
        Ok(true) => {
            if let Ok(snapshot) = crate::local_media::snapshot() {
                state.set_local_media(snapshot);
            }
            push(&app, &state);
            return Ok(());
        }
        Ok(false) if local_available => {
            return Err(AppError::Local(
                "This media app does not support repeat locally.".into(),
            ));
        }
        _ => {}
    }

    let client = state.ensure_client().ok_or_else(|| {
        AppError::Local("No local media session and Spotify is not connected.".into())
    })?;
    let device = device_id.or_else(|| state.settings().device_id.clone());
    client.repeat(&repeat_state, device.as_deref())?;

    if let Some(mut playback) = state.cache().playback {
        playback.repeat_state = repeat_state;
        state.set_playback(Some(playback));
    }
    push(&app, &state);
    Ok(())
}

/// Opens a URL (or Spotify deep link) in the user's default handler.
#[tauri::command]
pub fn open_spotify(app: AppHandle, url: Option<String>) -> AppResult<()> {
    // The plugin is registered in lib.rs; route through it so the URL is opened
    // by the OS rather than inside the webview.
    let target = match url {
        Some(url) if !url.trim().is_empty() => url,
        _ => {
            let state = app.state::<crate::state::AppState>();
            let cache = state.cache();
            cache
                .playback
                .and_then(|p| p.item)
                .map(|item| item.external_url)
                .filter(|url| !url.is_empty())
                .unwrap_or_else(|| "https://open.spotify.com".to_string())
        }
    };

    tauri_plugin_opener::open_url(target, None::<&str>)
        .map_err(|e| AppError::Local(format!("could not open Spotify: {e}")))
}

/// Returns the current settings.
#[tauri::command]
pub fn get_settings(app: AppHandle) -> AppResult<UiSettings> {
    let state = app.state::<crate::state::AppState>();
    Ok(UiSettings::from_settings(&state.settings()))
}

/// Partial settings update.
///
/// `patch` is the raw JSON the frontend sends; unknown keys are ignored so the
/// UI can evolve without a Rust release.
#[tauri::command]
pub fn update_settings(app: AppHandle, patch: serde_json::Value) -> AppResult<UiSettings> {
    let state = app.state::<crate::state::AppState>();

    let previous_mode = state.settings().mode;
    let previous_blur = state.settings().blur;

    let mut settings = state.settings();

    if let Some(value) = patch.get("transparency").and_then(|v| v.as_u64()) {
        settings.transparency = (value as u8).min(100);
    }
    if let Some(value) = patch.get("blur").and_then(|v| v.as_u64()) {
        settings.blur = (value as u8).min(100);
    }
    if let Some(value) = patch.get("solidSurface").and_then(|v| v.as_bool()) {
        settings.solid_surface = value;
    }
    if let Some(value) = patch.get("volume").and_then(|v| v.as_u64()) {
        settings.volume = (value as u8).min(100);
    }
    if let Some(value) = patch.get("alwaysOnTop").and_then(|v| v.as_bool()) {
        settings.always_on_top = value;
    }
    if let Some(value) = patch.get("startWithWindows").and_then(|v| v.as_bool()) {
        settings.start_with_windows = value;
    }
    if let Some(value) = patch.get("rememberPosition").and_then(|v| v.as_bool()) {
        settings.remember_position = value;
    }
    if let Some(value) = patch.get("showTray").and_then(|v| v.as_bool()) {
        settings.show_tray = value;
    }
    if let Some(value) = patch
        .get("hoverExpand")
        .or_else(|| patch.get("hoverMini"))
        .and_then(|v| v.as_bool())
    {
        settings.hover_expand = value;
    }
    if let Some(value) = patch.get("clientId").and_then(|v| v.as_str()) {
        // Trim silently: trailing whitespace is the single most common copy-paste
        // failure and it produces an opaque "invalid client".
        settings.client_id = value.trim().to_string();
    }
    if let Some(value) = patch.get("redirectPort").and_then(|v| v.as_u64()) {
        let port = value as u16;
        if port > 0 {
            settings.redirect_port = port;
        }
    }
    if let Some(value) = patch.get("mode").and_then(|v| v.as_str()) {
        settings.mode = WindowMode::from_str_lenient(value);
    }

    // -- Pocket: File Shelf --------------------------------------------------
    if let Some(value) = patch.get("shelfEnabled").and_then(|v| v.as_bool()) {
        settings.shelf_enabled = value;
    }
    if let Some(value) = patch.get("shelfMaxMib").and_then(|v| v.as_u64()) {
        // 0 means "no limit", so it is a legitimate value and must not be
        // filtered out by a `> 0` guard.
        settings.shelf_max_mib = value.min(u32::MAX as u64) as u32;
    }

    // -- Pocket: Clipboard Vault --------------------------------------------
    if let Some(value) = patch.get("clipboardEnabled").and_then(|v| v.as_bool()) {
        settings.clipboard_enabled = value;
    }
    if let Some(value) = patch.get("clipboardPrivate").and_then(|v| v.as_bool()) {
        settings.clipboard_private = value;
    }
    if let Some(value) = patch.get("clipboardRetentionDays").and_then(|v| v.as_u64()) {
        // Clamped to a sane band: 0 would mean "delete everything immediately",
        // which is a footgun rather than a preference.
        settings.clipboard_retention_days = (value.clamp(1, 365)) as u32;
    }
    if let Some(value) = patch.get("clipboardMaxEntries").and_then(|v| v.as_u64()) {
        settings.clipboard_max_entries = (value.clamp(10, 1000)) as u32;
    }
    if let Some(value) = patch
        .get("clipboardExcludedApps")
        .and_then(|v| v.as_array())
    {
        settings.clipboard_excluded_apps = value
            .iter()
            .filter_map(|entry| entry.as_str())
            .map(|entry| entry.trim().to_ascii_lowercase())
            .filter(|entry| !entry.is_empty())
            .collect();
    }

    let previous_client_id = state.settings().client_id.clone();
    let saved = state.update_settings(|s| *s = settings);

    // A changed client id invalidates any client built with the old one.
    if saved.client_id != previous_client_id {
        let _ = state.reset_client();
    }

    // Apply the side effects that must leave the Rust side.
    let _ = config::set_autostart(saved.start_with_windows);
    tray::set_tray_visible(&app, saved.show_tray);

    if let Some(window) = app.get_webview_window(MAIN_LABEL) {
        let _ = window.set_always_on_top(saved.always_on_top);
    }

    if saved.mode != previous_mode {
        tray::apply_mode(&app, saved.mode);
    }
    if saved.blur != previous_blur {
        crate::window::reapply_effects(&app);
    }

    push(&app, &state);
    Ok(UiSettings::from_settings(&saved))
}

/// Resizes the player to a mode and applies the matching window behaviour.
#[tauri::command]
pub fn set_window_mode(app: AppHandle, mode: String) -> AppResult<WindowMode> {
    let state = app.state::<crate::state::AppState>();
    let resolved = WindowMode::from_str_lenient(&mode);

    let mut settings = state.settings();
    settings.mode = resolved;
    let saved = state.update_settings(|s| *s = settings);

    // Clear any hover expansion before applying an explicit mode: the user picked
    // this mode, so a stale hover state must not immediately fight it.
    state.set_hover_expand(false);

    if let Some(window) = app.get_webview_window(MAIN_LABEL) {
        crate::window::apply_mode(&app, resolved);
        let _ = window.set_always_on_top(saved.always_on_top);
        let _ = window.show();
    }

    push(&app, &state);
    Ok(resolved)
}

/// Toggles always-on-top without touching the rest of the settings.
#[tauri::command]
pub fn set_always_on_top(app: AppHandle, enabled: bool) -> AppResult<()> {
    let state = app.state::<crate::state::AppState>();
    let saved = state.update_settings(|s| s.always_on_top = enabled);

    if let Some(window) = app.get_webview_window(MAIN_LABEL) {
        window
            .set_always_on_top(enabled)
            .map_err(|e| AppError::Local(format!("could not change always-on-top: {e}")))?;
    }

    let _ = saved;
    push(&app, &state);
    Ok(())
}

/// Turns the pointer-hover collapse on or off at runtime.
///
/// The frontend owns the *decision* (it knows when the pointer entered and left);
/// Rust owns the *geometry*. Keeping it this way means the webview never resizes
/// itself, so there is exactly one writer for the window rect.
#[tauri::command]
pub fn set_hover_mini(app: AppHandle, enabled: bool) -> AppResult<()> {
    let state = app.state::<crate::state::AppState>();
    state.set_hover_expand(enabled);
    apply_effective_mode(&app, &state);
    push(&app, &state);
    Ok(())
}

/// Resizes for the settings panel, which is not a player mode.
///
/// Settings is a full-height surface drawn inside the same window, so opening it
/// from the pill has to grow the window to the expanded height and closing it has
/// to give the height back. Keeping that here (rather than in the webview) means
/// the window and the panel are still driven by one side.
#[tauri::command]
pub fn set_settings_open(app: AppHandle, open: bool) -> AppResult<()> {
    let state = app.state::<crate::state::AppState>();
    state.set_settings_open(open);

    if open {
        // Collapse any hover state first: opening settings while the pointer is on
        // the pill would otherwise fight the hover watcher for the window height.
        state.set_hover_expand(false);
        crate::window::animate_to_mode(&app, WindowMode::Expanded);
    } else {
        // Closing hands control back to whatever mode is actually in force.
        apply_effective_mode(&app, &state);
    }

    push(&app, &state);
    Ok(())
}

/// Claims or releases the pointer so the hover watcher stands down.
///
/// Called by the webview when a drag starts or ends, and when a popover opens or
/// closes. Without it, starting a drag on the pill would shrink the window to mini
/// mid-gesture and the drag would lose its grip.
#[tauri::command]
pub fn set_pointer_claimed(app: AppHandle, claimed: bool) -> AppResult<()> {
    let state = app.state::<crate::state::AppState>();
    state.set_pointer_claimed(claimed);
    if !claimed {
        // Releasing is the moment hover should re-evaluate; the watcher would get
        // there within 16 ms anyway, but doing it here keeps the handover exact.
        //
        // A panel that is still open owns the window height. This command is
        // called from several places that have nothing to do with panels —
        // `window.blur` fires it on every alt-tab — so releasing the pointer must
        // not resize a 468 px Pocket down to the 108 px player. Doing so cut the
        // file list off mid-read, which is what the user saw.
        apply_effective_mode(&app, &state);
    }
    Ok(())
}

/// Opens or closes Pocket.
///
/// Pocket is a *page*, not a taller window. It occupies the same rect as the
/// pill (416x108) and the island slides sideways between the player and Pocket,
/// so **no resize happens here at all**.
///
/// That is a deliberate reversal of the earlier design, where Pocket grew the
/// window to 468 px. Growing the window for a panel meant the hover watcher, the
/// `window.blur` handler and the panel all had an opinion about the window's
/// height, and they disagreed: the list got cut off when the pointer left. With a
/// fixed pill-height rect there is nothing to fight over.
///
/// The pointer is still claimed while Pocket is open. Not to protect a height —
/// there is none — but because the hover watcher would otherwise collapse
/// pill → mini underneath a page the user is reading.
#[tauri::command]
pub fn set_pocket_open(app: AppHandle, open: bool) -> AppResult<()> {
    let state = app.state::<crate::state::AppState>();
    state.set_pocket_open(open);

    if open {
        state.set_hover_expand(false);
        // Pocket owns the pointer while it is open; the hover watcher must not
        // collapse the island underneath the page the user is reading.
        state.set_pointer_claimed(true);
        // No resize. The rect is already the pill's.
        crate::window::animate_to_mode(&app, state.effective_mode());
    } else {
        // Landing back on the player.
        //
        // Re-arm hover expansion instead of dropping straight to the saved mode.
        // With `mode = mini`, `effective_mode()` returns Mini the instant
        // `pocket_open` goes false, so the window shrank from 416 to 248 px while
        // the cursor was still sitting on the island — and then the hover watcher
        // immediately grew it back, which is the "shrinks and grows again" flicker.
        //
        // Re-arming makes `effective_mode()` return Pill, so the window stays the
        // size it already was. The hover watcher then owns the collapse: it only
        // shrinks to mini once the pointer has actually left, which is the
        // behaviour asked for — same size first, small a few seconds later.
        //
        // This is a no-op for a saved mode of `pill` or `expanded`: those already
        // resolve to their own size, and the hover watcher stands down for them.
        state.set_hover_expand(true);
        state.set_pointer_claimed(false);
        apply_effective_mode(&app, &state);
    }

    push(&app, &state);
    Ok(())
}

/// Enters or leaves the file-drop surface.
///
/// ## Why the webview does not own this
///
/// A drop from Explorer is a native OLE drag. The webview learns about it through
/// Tauri's `tauri://drag-enter` / `drag-leave` events, but it must not decide the
/// window's size: the same hit-test that drives hover lives in Rust, and if two
/// sides wrote the rect the window would fight itself. So the webview reports the
/// gesture and Rust owns the geometry — the same split the hover collapse uses.
///
/// ## What the flag does
///
/// While `active` is true the hover watcher treats the pointer as claimed, so it
/// cannot collapse the window out from under the drop target. The webview draws
/// the target under a taller window; a cancelled drag animates back to the mode
/// that was in force.
#[tauri::command]
pub fn set_drop_target(app: AppHandle, active: bool) -> AppResult<()> {
    let state = app.state::<crate::state::AppState>();

    // Ignore a redundant transition: `drag-enter` can fire for each child
    // element in some webviews, and re-animating on every one would stutter.
    if state.drop_target() == active {
        return Ok(());
    }

    state.set_drop_target(active);

    if active {
        // Hover would otherwise re-evaluate against the *old* pill rect while the
        // window grows, and the flicker that produces is visible.
        state.set_pointer_claimed(true);
        crate::window::animate_to_drop(&app, true);
    } else {
        crate::window::animate_to_drop(&app, false);
        state.set_pointer_claimed(false);
        apply_effective_mode(&app, &state);
    }

    push(&app, &state);
    Ok(())
}

/// Resizes the window to whatever mode the UI should be showing right now.
///
/// Both the hover transition and the tray land here, so the size the OS window
/// ends up with is always the size of the panel the webview is drawing.
fn apply_effective_mode<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    state: &crate::state::AppState,
) {
    // A panel that is open owns the window height. This function is reached from
    // every "the pointer is free again" path — `set_pointer_claimed(false)`,
    // `set_drop_target(false)`, closing the drop surface — and none of those mean
    // "the panel closed". Resizing to the player mode here is what collapsed an
    // open Pocket from 468 px to 108 px and cut the file list off.
    if let Some(height) = state.panel_height() {
        crate::window::animate_to_height(app, height);
        return;
    }

    let mode = state.effective_mode();
    crate::window::apply_mode(app, mode);
}

/// Reads the window's current outer frame in *logical* pixels.
///
/// The frontend needs this to tween between mode geometries: it has to know where
/// the window is starting from before it can animate to where it is going.
#[tauri::command]
pub fn window_get_frame(app: AppHandle) -> AppResult<Frame> {
    let window = app
        .get_webview_window(MAIN_LABEL)
        .ok_or_else(|| AppError::Local("main window is missing".into()))?;

    let position = window
        .outer_position()
        .map_err(|e| AppError::Local(format!("could not read position: {e}")))?;
    let size = window
        .outer_size()
        .map_err(|e| AppError::Local(format!("could not read size: {e}")))?;
    let scale = window.scale_factor().unwrap_or(1.0);

    Ok(Frame {
        x: position.x as f64 / scale,
        y: position.y as f64 / scale,
        width: size.width as f64 / scale,
        height: size.height as f64 / scale,
    })
}

/// Moves and resizes the window in one step.
///
/// Both halves have to be applied together: resizing alone leaves a shrinking pill
/// pinned to its old left edge instead of staying centred, and the two calls racing
/// each other is what made mode changes look like they snapped.
#[tauri::command]
pub fn window_set_frame(app: AppHandle, x: f64, y: f64, width: f64, height: f64) -> AppResult<()> {
    if width < 120.0 || height < 40.0 || width > 2000.0 || height > 2000.0 {
        return Err(AppError::Local(format!(
            "Refusing implausible window size {width}x{height}"
        )));
    }
    let window = app
        .get_webview_window(MAIN_LABEL)
        .ok_or_else(|| AppError::Local("main window is missing".into()))?;

    window
        .set_size(tauri::LogicalSize::new(width, height))
        .map_err(|e| AppError::Local(format!("could not resize the window: {e}")))?;
    window
        .set_position(tauri::PhysicalPosition::new(
            x.round() as i32,
            y.round() as i32,
        ))
        .map_err(|e| AppError::Local(format!("could not move the window: {e}")))?;
    Ok(())
}

/// Resizes the player to an explicit logical size.
///
/// The frontend calls this whenever the mode changes so the OS window always
/// matches the panel being drawn. Keeping geometry in Rust avoids a class of bugs
/// where the webview and the window disagree after a state snapshot.
#[tauri::command]
pub fn window_set_size(app: AppHandle, width: f64, height: f64) -> AppResult<()> {
    if width < 120.0 || height < 40.0 || width > 2000.0 || height > 2000.0 {
        return Err(AppError::Local(format!(
            "Refusing implausible window size {width}x{height}"
        )));
    }
    let window = app
        .get_webview_window(MAIN_LABEL)
        .ok_or_else(|| AppError::Local("main window is missing".into()))?;
    window
        .set_size(tauri::LogicalSize::new(width, height))
        .map_err(|e| AppError::Local(format!("could not resize the window: {e}")))?;
    Ok(())
}

/// Hides the player to the tray.
#[tauri::command]
pub fn hide_window(app: AppHandle) -> AppResult<()> {
    if let Some(window) = app.get_webview_window(MAIN_LABEL) {
        window
            .hide()
            .map_err(|e| AppError::Local(format!("could not hide the window: {e}")))?;
    }
    Ok(())
}

/// Shows and focuses the player.
#[tauri::command]
pub fn show_window(app: AppHandle) -> AppResult<()> {
    tray::show_window(&app);
    Ok(())
}

/// Quits the app, persisting geometry first.
#[tauri::command]
pub fn quit_app(app: AppHandle) -> AppResult<()> {
    crate::window::persist_geometry(&app);
    app.exit(0);
    Ok(())
}

/// Returns the persisted window position, if any.
#[tauri::command]
pub fn get_saved_geometry() -> Option<Geometry> {
    config::load_geometry()
}

// Five commands used to live here without being registered in `lib.rs`:
// get_current_playback, get_profile, save_geometry, is_configured and
// reload_settings. `#[tauri::command]` alone does nothing — the generate_handler!
// list is what exposes them. They were also unreachable from the frontend, which
// still declared `currentPlayback()` and `saveGeometry()` wrappers, so the first
// call would have failed with "command not found". Removed rather than wired up:
// nothing needed them, and a registered-but-unused command is an attack surface
// with no caller to justify it.
