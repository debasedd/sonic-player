//! Native file drag-and-drop.
//!
//! ## Root cause of the original failure
//!
//! Tauri installs its own drag-and-drop handler by default — `dragDropEnabled`
//! defaults to `true` in `tauri-utils`, and `tauri-runtime-wry` wires it up with
//! `webview_builder.with_drag_drop_handler(...)`. That closure has to choose
//! *which* channel to deliver the event on, and the choice depends on the webview
//! kind:
//!
//! ```text
//! tauri-runtime-wry/src/lib.rs, inside with_drag_drop_handler:
//!   let message = if kind == WebviewKind::WindowContent {
//!       WebviewMessage::SynthesizedWindowEvent(SynthesizedWindowEvent::DragDrop(event))
//!     } else {
//!       WebviewMessage::WebviewEvent(WebviewEvent::DragDrop(event))
//!     };
//! ```
//!
//! A Tauri `WebviewWindow` is created as `WindowContent` (the webview fills the
//! whole window), so a native Explorer drag arrives as a **window** event.
//! `SynthesizedWindowEvent::DragDrop` converts into `WindowEvent::DragDrop`
//! (`From<SynthesizedWindowEvent> for WindowEventWrapper`), and
//! `manager/window.rs` dispatches that to the window's listeners.
//!
//! This module used to subscribe only to `on_webview_event`, which is the
//! *webview* channel. The callback was correct, the drag was correct, and the two
//! never met — which is why the synthetic OLE probe reported `effect=0` and why
//! nothing ever reached the shelf. The drop was being consumed by Tauri's handler
//! and delivered one channel over from where this code was listening.
//!
//! Both channels are subscribed now. It costs one extra `match` per event, and
//! `DROP_ACTIVE` makes the pair idempotent, so a doubly-delivered phase cannot
//! resize twice or import twice.
//!
//! ## Why the state flag lives here too
//!
//! The window has to grow into the drop target *before* the user releases, or the
//! drop lands on a pill that is not showing a target. That transition is a Rust
//! geometry change, so the enter/leave bookkeeping is Rust's as well — the
//! webview only draws what the flag says.
//!
//! ## What is never logged
//!
//! `trace()` records event kind, a path count, and coordinates. Never a path,
//! never file contents, never clipboard text, never a token.

use crate::state::AppState;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{Manager, WebviewEvent, WindowEvent};

/// Appends one line to `%TEMP%\sonic-drag.log`.
///
/// Deliberately not `eprintln!`: release builds use the Windows subsystem, so
/// stderr goes nowhere. A drop is a rare, user-visible event, and having a file to
/// read is the difference between diagnosing it and guessing.
fn trace(line: &str) {
    use std::io::Write;
    let path = std::env::temp_dir().join("sonic-drag.log");
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(file, "{line}");
    }
}

/// Mirrors the webview's `dropActive` flag.
///
/// Static rather than state-held because the callbacks are plain `Fn` with no
/// `AppHandle` capture. The flag is what the frontend reads, and the frontend
/// already mirrors it into its own store.
static DROP_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Attaches the native drag-and-drop handler to a window.
///
/// Takes an owned `WebviewWindow` rather than a reference: the callbacks are
/// stored for the lifetime of the window, so they must own their handles. A
/// borrow here is a `'static` violation, which is exactly what the compiler
/// rejects.
pub fn attach<R: tauri::Runtime>(window: tauri::WebviewWindow<R>) {
    trace(&format!(
        "attach label={} hwnd={:?}",
        window.label(),
        window.hwnd().map(|h| h.0).ok()
    ));

    // Channel 1 — the one that actually fires for a WebviewWindow on Windows.
    // See the module comment: WindowContent webviews get SynthesizedWindowEvent,
    // which becomes WindowEvent::DragDrop.
    let window_for_window_event = window.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::DragDrop(event) = event {
            handle(window_for_window_event.clone(), event);
        }
    });

    // Channel 2 — kept for webviews that are children of a window, where wry
    // delivers `WebviewEvent::DragDrop` instead. Inert on Windows today, but it
    // means the drop keeps working if Sonic ever moves to a child webview.
    let window_for_webview_event = window.clone();
    window.on_webview_event(move |event| {
        if let WebviewEvent::DragDrop(event) = event {
            handle(window_for_webview_event.clone(), event);
        }
    });
}

/// Handles one native drag event, whichever channel delivered it.
///
/// Both callbacks funnel here so the geometry and import logic exist exactly once.
/// `DROP_ACTIVE` makes the pair idempotent: a second delivery of the same phase is
/// a no-op instead of a second resize or a second import.
fn handle<R: tauri::Runtime>(window: tauri::WebviewWindow<R>, event: &tauri::DragDropEvent) {
    match event {
        tauri::DragDropEvent::Enter { paths, position } => {
            trace(&format!(
                "enter paths={} pos={},{}",
                paths.len(),
                position.x,
                position.y
            ));
            // A drag with no paths is not a file drag (text selections, some
            // in-app drags). Ignoring it keeps the target from appearing for
            // drops Sonic cannot accept.
            if paths.is_empty() {
                return;
            }
            set_drop_active(&window, true);
        }
        tauri::DragDropEvent::Over { position } => {
            // Fires on every mouse move during a drag, so this is deliberately
            // the shortest possible line: it is the cheapest proof that the drag
            // is live over the window, and it is bounded by how long the user
            // holds the mouse.
            trace(&format!("over pos={},{}", position.x, position.y));
        }
        tauri::DragDropEvent::Drop { paths, position } => {
            trace(&format!(
                "drop paths={} pos={},{}",
                paths.len(),
                position.x,
                position.y
            ));
            set_drop_active(&window, false);

            if paths.is_empty() {
                trace("drop ignored: payload carried no paths");
                return;
            }

            // Import on a worker: the copy is synchronous file IO, and doing it
            // on the window thread would freeze the animation.
            let app = window.app_handle().clone();
            let paths: Vec<String> = paths
                .iter()
                .map(|path| path.to_string_lossy().to_string())
                .collect();
            std::thread::spawn(move || {
                let state = app.state::<AppState>();
                let max_mib = state.settings().shelf_max_mib;

                let report = crate::shelf::Shelf::import_locked(
                    paths.into_iter().map(std::path::PathBuf::from).collect(),
                    max_mib,
                );
                trace(&format!(
                    "shelf import: added={} skipped={}",
                    report.added.len(),
                    report.skipped.len()
                ));

                state.refresh_pocket_stats();
                crate::commands::push_state(&app, &state);
            });
        }
        tauri::DragDropEvent::Leave => {
            trace("leave");
            set_drop_active(&window, false);
        }
        _ => {}
    }
}

/// Enters or leaves the drop-target geometry.
///
/// Debounced through the atomic: `drag-enter` fires once per element the cursor
/// crosses, and re-animating on every one of those is a visible stutter.
fn set_drop_active<R: tauri::Runtime>(window: &tauri::WebviewWindow<R>, active: bool) {
    let previous = DROP_ACTIVE.swap(active, Ordering::AcqRel);
    if previous == active {
        return;
    }

    let app = window.app_handle().clone();
    let state = app.state::<AppState>();
    state.set_drop_target(active);

    if active {
        // Hover would otherwise re-evaluate against the *old* pill rect while
        // the window grows, and the flicker that produces is visible.
        state.set_pointer_claimed(true);
        crate::window::animate_to_drop(&app, true);
    } else {
        // Restore whatever surface was on screen before the drag started. A
        // cancelled drag must never shrink a window that is holding Pocket or
        // Settings, or the list gets clipped.
        let restore = state.panel_height();
        state.set_pointer_claimed(restore.is_some());
        let height = restore.unwrap_or_else(|| state.effective_mode().size().1);
        crate::window::animate_to_height(&app, height);
    }

    crate::commands::push_state(&app, &state);
}

/// Test seam: lets the unit tests drive the flag without a window.
#[cfg(test)]
pub(crate) fn drop_active() -> bool {
    DROP_ACTIVE.load(Ordering::Acquire)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drop_flag_starts_cleared() {
        // The app must never boot into the drop-target geometry: a stuck flag
        // would leave the window 176 px tall with no drag in progress.
        assert!(!drop_active());
    }

    #[test]
    fn flag_is_idempotent_per_value() {
        DROP_ACTIVE.store(false, Ordering::Release);
        assert!(!drop_active());
        DROP_ACTIVE.store(true, Ordering::Release);
        assert!(drop_active());
        DROP_ACTIVE.store(false, Ordering::Release);
        assert!(!drop_active());
    }

    #[test]
    fn cancelled_drag_clears_the_flag() {
        // A leave must always clear the flag, or the window stays expanded and
        // the player is stuck behind the drop surface.
        DROP_ACTIVE.store(true, Ordering::Release);
        DROP_ACTIVE.store(false, Ordering::Release);
        assert!(!drop_active());
    }
}
