//! Pocket commands — File Shelf and Clipboard Vault.
//!
//! Split from `commands.rs` because Pocket is a self-contained feature area: the
//! player commands are about playback, these are about files and clipboard data.
//! Both go through the same `AppState`, so `push()` still emits one snapshot.
//!
//! ## Drag-out
//!
//! `shelf_drag_out` is a genuine OLE drag: it hands Windows a list of real paths
//! via `tauri-plugin-drag`, and Explorer (or any other app) receives usable
//! files. The stored copy is not moved or deleted by the drag — dragging out is
//! a copy, which is why `drag::DragMode::Copy` is used and why the shelf still
//! owns the file afterwards.

use crate::config;
use crate::error::{AppError, AppResult};
use crate::shelf::{ImportReport, Shelf, ShelfEntry};
use crate::state::AppState;
use tauri::{AppHandle, Manager};

/// Pushes a fresh snapshot after a Pocket mutation.
fn push(app: &AppHandle, state: &AppState) {
    state.refresh_pocket_stats();
    crate::commands::push_state(app, state);
}

// -- File Shelf --------------------------------------------------------------

/// Imports absolute paths (from the native file drop) into the shelf.
#[tauri::command]
pub fn shelf_import(app: AppHandle, paths: Vec<String>) -> AppResult<ImportReport> {
    let state = app.state::<AppState>();
    if !state.settings().shelf_enabled {
        return Err(AppError::Local(
            "The File Shelf is turned off. Enable it in Pocket settings.".into(),
        ));
    }

    let max_mib = state.settings().shelf_max_mib;
    let paths: Vec<std::path::PathBuf> = paths.into_iter().map(Into::into).collect();

    let mut shelf = Shelf::load();
    let report = shelf.import_paths(paths, max_mib);
    push(&app, &state);
    Ok(report)
}

/// Lists the shelf contents.
#[tauri::command]
pub fn shelf_list() -> AppResult<Vec<ShelfEntry>> {
    Ok(Shelf::load().entries())
}

/// Pins or unpins an entry.
#[tauri::command]
pub fn shelf_set_pinned(app: AppHandle, id: String, pinned: bool) -> AppResult<()> {
    let state = app.state::<AppState>();
    Shelf::load().set_pinned(&id, pinned)?;
    push(&app, &state);
    Ok(())
}

/// Deletes Sonic's copy of a file. The original the user dropped is untouched.
#[tauri::command]
pub fn shelf_remove(app: AppHandle, id: String) -> AppResult<()> {
    let state = app.state::<AppState>();
    Shelf::load().remove(&id)?;
    push(&app, &state);
    Ok(())
}

/// Removes every unpinned entry. Returns how many were deleted.
#[tauri::command]
pub fn shelf_clear(app: AppHandle) -> AppResult<usize> {
    let state = app.state::<AppState>();
    let removed = Shelf::load().clear_unpinned()?;
    push(&app, &state);
    Ok(removed)
}

/// Opens a stored file with its default Windows application.
#[tauri::command]
pub fn shelf_open(path: String) -> AppResult<()> {
    tauri_plugin_opener::open_path(path, None::<&str>)
        .map_err(|e| AppError::Local(format!("could not open that file: {e}")))
}

/// Shows a stored file in File Explorer, selected.
#[tauri::command]
pub fn shelf_reveal(path: String) -> AppResult<()> {
    tauri_plugin_opener::reveal_item_in_dir(path)
        .map_err(|e| AppError::Local(format!("could not reveal that file: {e}")))
}

/// Starts a native drag of the given stored files.
///
/// `paths` are Sonic's own copies. The drag is a copy operation, so dropping on
/// the desktop or in Explorer produces new files there and leaves the shelf
/// intact — the user is moving a copy out, not removing the original.
///
/// ## Why this blocks the calling thread
///
/// `drag::start_drag` runs the OLE modal loop on Windows: it does not return
/// until the user releases the mouse. That is why the command is synchronous and
/// why the webview is told to expect no reply until the gesture ends. Running it
/// on a worker would put the OLE loop on a thread that owns no window.
#[tauri::command]
pub fn shelf_drag_out(
    app: AppHandle,
    window: tauri::WebviewWindow,
    paths: Vec<String>,
    ghost: Option<String>,
) -> AppResult<()> {
    if paths.is_empty() {
        return Ok(());
    }

    let items: Vec<std::path::PathBuf> = paths.into_iter().map(Into::into).collect();

    // The drag ghost is a small PNG rendered by the frontend from the file's own
    // thumbnail where one exists. Without it Windows draws a generic drag image,
    // which looks broken next to the rest of the surface. An empty ghost is
    // still valid: the `drag` crate then uses its default icon.
    let image = match ghost.as_deref() {
        Some(data) if data.starts_with("data:image/png;base64,") => {
            let encoded = data.trim_start_matches("data:image/png;base64,");
            let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded)
                .map_err(|e| AppError::Local(format!("bad drag image: {e}")))?;
            drag::Image::Raw(bytes)
        }
        _ => drag::Image::Raw(Vec::new()),
    };

    // OLE drag must run where the window lives.
    let (tx, rx) = std::sync::mpsc::channel::<AppResult<()>>();
    let handle = app.clone();
    app.run_on_main_thread(move || {
        let result = drag::start_drag(
            &window,
            drag::DragItem::Files(items),
            image,
            |_result, _cursor| {},
            drag::Options::default(),
        )
        .map_err(|e| AppError::Local(format!("could not start the drag: {e}")));
        let _ = tx.send(result);
    })
    .map_err(|e| AppError::Local(format!("could not reach the main thread: {e}")))?;

    // The modal loop ends when the user drops or cancels. No timeout: a drag can
    // legitimately stay open for as long as the user holds the mouse.
    let _ = handle;
    rx.recv()
        .map_err(|_| AppError::Local("the drag ended without a result".into()))?
}

// -- Clipboard Vault ---------------------------------------------------------

/// Returns the vault contents.
#[tauri::command]
pub fn clipboard_list() -> AppResult<Vec<crate::clipboard::ClipboardEntry>> {
    Ok(crate::clipboard::ClipboardVault::load().entries())
}

/// Restores an entry to the real Windows clipboard.
#[tauri::command]
pub fn clipboard_restore(app: AppHandle, id: String) -> AppResult<()> {
    crate::clipboard_sys::restore_entry(&id)?;
    let state = app.state::<AppState>();
    push(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn clipboard_set_pinned(app: AppHandle, id: String, pinned: bool) -> AppResult<()> {
    let state = app.state::<AppState>();
    crate::clipboard::ClipboardVault::load().set_pinned(&id, pinned)?;
    push(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn clipboard_remove(app: AppHandle, id: String) -> AppResult<()> {
    let state = app.state::<AppState>();
    crate::clipboard::ClipboardVault::load().remove(&id)?;
    state.bump_clipboard_revision();
    push(&app, &state);
    Ok(())
}

/// Clears the whole vault. Explicit user action only.
#[tauri::command]
pub fn clipboard_clear(app: AppHandle) -> AppResult<usize> {
    let state = app.state::<AppState>();
    let removed = crate::clipboard::ClipboardVault::load().clear_all()?;
    state.bump_clipboard_revision();
    push(&app, &state);
    Ok(removed)
}

/// Forces a capture pass right now.
///
/// The background watcher polls the sequence number every ~900 ms, so a copy is
/// picked up almost immediately. This command exists for the manual "check now"
/// affordance and for the empty-state button, which would otherwise look dead
/// for the better part of a second.
#[tauri::command]
pub fn clipboard_capture_now(app: AppHandle) -> AppResult<bool> {
    let state = app.state::<AppState>();
    let captured = crate::clipboard_sys::capture_once(&state);
    if captured.is_some() {
        push(&app, &state);
    }
    Ok(captured.is_some())
}

/// Runs the retention pass and deletes orphaned image files.
#[tauri::command]
pub fn clipboard_prune(app: AppHandle) -> AppResult<usize> {
    let state = app.state::<AppState>();
    let mut vault = crate::clipboard::ClipboardVault::load();
    vault.enforce_limits()?;
    let orphans = vault.prune_orphan_images();
    push(&app, &state);
    Ok(orphans)
}

/// Reports how much disk the shelf and the vault are using.
#[tauri::command]
pub fn pocket_usage() -> AppResult<config::PocketUsage> {
    let shelf = Shelf::load();
    let clipboard = crate::clipboard::ClipboardVault::load();
    Ok(config::PocketUsage {
        shelf_count: shelf.entries().len() as u32,
        shelf_bytes: shelf.total_bytes(),
        clipboard_count: clipboard.entries().len() as u32,
        clipboard_bytes: clipboard.total_bytes(),
        shelf_path: config::shelf_dir().to_string_lossy().to_string(),
        clipboard_path: config::clipboard_dir().to_string_lossy().to_string(),
    })
}
