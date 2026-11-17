//! Background clipboard watcher.
//!
//! ## Why polling and not a clipboard listener
//!
//! A `WM_CLIPBOARDUPDATE` listener needs a message-only window with a lifetime
//! that survives Tauri's event loop. `GetClipboardSequenceNumber` is one
//! `user32` call that returns a counter incremented on every clipboard change,
//! so a poll is both simpler and cheaper than owning a hidden window: when the
//! clipboard is idle the cost is a single syscall per interval.
//!
//! ## Why this can sleep longer than the interaction path
//!
//! Nothing the user does in Sonic waits on a capture. A 900 ms poll means a
//! copied snippet shows up "about now", which is well inside what a clipboard
//! history needs to feel live, and it is the difference between 1 wakeup/s and
//! 60 for a feature that is off by default.
//!
//! ## Privacy gate
//!
//! The gate lives in `clipboard_sys::capture_once`, not here, so there is
//! exactly one place where capture can be suppressed. This module only decides
//! when to call it.

use crate::state::AppState;
use std::time::Duration;
use tauri::Manager;

/// How often the clipboard sequence number is read while capture is enabled.
const POLL: Duration = Duration::from_millis(900);

/// How long to sleep when capture is disabled or Private Mode is on.
///
/// Longer than [`POLL`] on purpose: a disabled vault should cost almost
/// nothing, but the toggle has to be noticed without restarting Sonic.
const PARK: Duration = Duration::from_millis(1500);

pub fn spawn(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        // The sequence number the vault last saw. Starting at 0 guarantees the
        // first pass captures whatever is already on the clipboard when capture
        // is switched on, rather than waiting for the next copy.
        let mut last_sequence: Option<u32> = None;

        loop {
            let state = app.state::<AppState>();

            #[cfg(windows)]
            let active = {
                let settings = state.settings();
                settings.clipboard_enabled && !settings.clipboard_private
            };
            #[cfg(not(windows))]
            let active = false;

            if !active {
                // Drop the remembered sequence so re-enabling captures the
                // current clipboard instead of ignoring it as "already seen".
                last_sequence = None;
                std::thread::sleep(PARK);
                continue;
            }

            #[cfg(windows)]
            {
                let current = crate::clipboard_sys::sequence_number();

                // Keep the indicator honest even when nothing new arrives.
                if let Some(sequence) = current {
                    state.set_clipboard_sequence(sequence);
                }

                let changed = match (current, last_sequence) {
                    (Some(sequence), Some(seen)) => sequence != seen,
                    (Some(_), None) => true,
                    (None, _) => false,
                };

                if changed {
                    if let Some(entry) = crate::clipboard_sys::capture_once(&state) {
                        crate::window::diag(&format!(
                            "clipboard captured: kind={:?} source={}",
                            entry.kind, entry.source
                        ));

                        // A capture is invisible to the UI unless a snapshot is
                        // pushed: `set_clipboard_dirty` bumps the revision, but
                        // the revision only reaches the webview inside a
                        // `UiState`. Without this the vault list stayed stale
                        // until the user switched tabs and forced a refresh.
                        state.refresh_pocket_stats();
                        crate::commands::push_state(&app, &state);

                        last_sequence = current;
                        std::thread::sleep(POLL);
                        continue;
                    }
                    // Capture declined (sensitive pattern, excluded app, empty).
                    // Remember the sequence anyway, or every pass would retry the
                    // same content and burn a clipboard read per interval.
                    last_sequence = current;
                }
            }

            std::thread::sleep(POLL);
        }
    });
}
