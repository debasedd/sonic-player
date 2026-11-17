//! Pointer-hover expansion: mini → pill while touched, back to mini on leave.
//!
//! ## The gesture
//!
//! At rest Sonic is the small strip (mini, 248x60). Touching it with the pointer
//! grows it into the full pill (416x108) with artwork, title, progress and the
//! transport; moving away shrinks it back.
//!
//! ## Why this lives in Rust and not in the webview
//!
//! A `pointerenter` handler in the webview cannot drive this reliably. The window
//! changes size under the pointer, so the webview's own notion of "is the pointer
//! over me" is stale the moment the animation starts -- and while the window is
//! 248 px wide, a cursor that was over the pill's old right edge is outside the
//! window entirely, so the webview receives no pointer events at all and can never
//! learn that the pointer left.
//!
//! So the hit test needs a source that keeps reporting regardless of window size:
//! the OS cursor, polled from Rust against the window's own rect.
//!
//! ## Hysteresis
//!
//! Enter uses the pill's footprint inflated by [`ENTER_MARGIN`], so it fires as soon
//! as the pointer is on the strip. Leave uses the pill's footprint inflated by
//! [`LEAVE_MARGIN`]. The pill rect fully contains the mini rect, so the two tests
//! cannot disagree about the same pixel -- a cursor inside one is inside the other --
//! which is what makes the state settle instead of flickering.
//!
//! ## Single writer
//!
//! This module animates the window itself (see [`crate::window::animate_to_mode`]).
//! The webview never resizes the window, which removes the whole class of
//! "DOM and window disagree" bugs.

use crate::config::WindowMode;
use crate::state::AppState;
use crate::window::{animate_to_mode, MAIN_LABEL};
use std::time::Duration;
use tauri::Manager;

/// 30 Hz is enough for OS-cursor hover while avoiding 144 wakeups/s.
const POLL: Duration = Duration::from_millis(33);

/// Extra logical pixels around the pill that still count as "on the player" while
/// expanded. Leaving has to feel deliberate rather than twitchy.
const LEAVE_MARGIN: f64 = 28.0;
const LEAVE_MARGIN_Y: f64 = 20.0;

/// Extra logical pixels around the strip that will trigger the expansion.
const ENTER_MARGIN: f64 = 12.0;
const ENTER_MARGIN_Y: f64 = 8.0;

/// Spawns the watcher. Cheap to call once at startup; the thread parks whenever the
/// feature is off, so an idle Sonic with hover disabled costs one wakeup per second
/// and nothing else.
pub fn spawn(app: tauri::AppHandle) {
    std::thread::spawn(move || loop {
        let state = app.state::<AppState>();

        // Park when the feature is off, or when an explicit non-mini mode is in
        // force: hover must never fight a mode the user chose on purpose.
        if !state.settings().hover_expand || state.settings().mode != WindowMode::Mini {
            if state.is_hover_expand() {
                state.set_hover_expand(false);
                crate::player::emit_state(&app, &state);
                animate_to_mode(&app, state.effective_mode());
            }
            std::thread::sleep(Duration::from_millis(250));
            continue;
        }

        // A drag, a popover or the settings panel owns the pointer. Expanding
        // underneath an open menu would tear it away from the cursor, and the
        // settings window is 548 px tall so neither rect would describe it.
        if pointer_is_claimed(&app) {
            std::thread::sleep(POLL);
            continue;
        }

        match hit_test(&app) {
            Some(inside) => {
                let expanded = state.is_hover_expand();

                if inside != expanded {
                    crate::window::diag(&format!(
                        "hover transition: inside={inside}, expanded={expanded}"
                    ));
                    state.set_hover_expand(inside);
                    // Emit state so webview begins drawing the new mode
                    crate::player::emit_state(&app, &state);
                    // Animate the OS window frame to match
                    animate_to_mode(&app, state.effective_mode());
                }
            }
            None => {}
        }

        std::thread::sleep(POLL);
    });
}

/// True when the pointer is within the rect that should trigger a change.
///
/// The rect is always the pill's footprint, because that is the size the pointer
/// has to reach (while collapsed) or leave (while expanded). The window's own rect
/// is used only for its centre, so this stays correct at any size and DPI.
///
/// Returns `None` when the geometry cannot be read, which the caller treats as
/// "no change" rather than "not hovering".
fn hit_test(app: &tauri::AppHandle) -> Option<bool> {
    let window = app.get_webview_window(MAIN_LABEL)?;
    if !window.is_visible().unwrap_or(false) {
        return Some(false);
    }

    let expanded = app.state::<AppState>().is_hover_expand();
    let (margin, margin_y) = if expanded {
        (LEAVE_MARGIN, LEAVE_MARGIN_Y)
    } else {
        (ENTER_MARGIN, ENTER_MARGIN_Y)
    };

    let scale = window.scale_factor().unwrap_or(1.0);
    let cursor = app.cursor_position().ok()?;
    let outer = window.outer_position().ok()?;
    let size = window.outer_size().ok()?;

    // The player is one floating object: the horizontal centre is the invariant,
    // not the left edge.
    let centre_x = outer.x as f64 + size.width as f64 / 2.0;
    let top_y = outer.y as f64;

    let (target_w, target_h) = if expanded {
        WindowMode::Pill.size()
    } else {
        WindowMode::Mini.size()
    };
    let half_w = target_w * scale / 2.0;
    let height = target_h * scale;

    let inside_x = (cursor.x - centre_x).abs() <= half_w + margin * scale;
    let inside_y =
        (cursor.y >= top_y - margin_y * scale) && (cursor.y <= top_y + height + margin_y * scale);

    Some(inside_x && inside_y)
}

/// True while something is using the pointer and hover should stand down.
///
/// A drag is the important case: expanding mid-drag would yank the window out from
/// under the user's hand. The webview publishes this because only it knows a drag
/// or a popover is in progress.
///
/// The two panels are listed here as well as in `pointer_claimed`, deliberately.
/// `set_pointer_claimed(false)` is called from several places in the webview —
/// `window.blur` alone fires it whenever the user alt-tabs or clicks another
/// window — and each of those paths used to resize the window back to the player
/// mode. With Pocket open that meant 468 px collapsing to 108 px and the file list
/// being cut off, which is exactly the bug this list prevents: the panel's height
/// is not the hover watcher's to reclaim.
fn pointer_is_claimed(app: &tauri::AppHandle) -> bool {
    let state = app.state::<AppState>();
    state.pointer_claimed() || state.settings_open() || state.pocket_open()
}
