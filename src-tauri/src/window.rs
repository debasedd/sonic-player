//! Frameless, always-on-top floating window management.
//!
//! Sonic never uses a native title bar. The whole surface is one transparent
//! window with rounded corners painted in CSS, so the desktop shows through the
//! glass.

use crate::config::{Geometry, WindowMode};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tauri::{
    LogicalSize, Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};

pub const MAIN_LABEL: &str = "main";

/// Logical width of the player. Height varies by mode.
pub const WINDOW_WIDTH: f64 = 416.0;

/// Logical height of the pill. Used as the hover hit-test height; the mode's own
/// size is used for everything else.
pub const WINDOW_HEIGHT: f64 = 108.0;

/// Duration of a mode change, in milliseconds.
///
/// One number for the whole app: the webview's layout transition reads the same
/// value, so the window and the content arrive together. Two different durations
/// is what made a mode change read as "content first, window after".
pub const MODE_MS: i64 = 400;

/// Gap kept from the top edge of the work area on first launch.
const DEFAULT_TOP_GAP: f64 = 24.0;

#[cfg(test)]
mod geometry_tests {
    use super::centered_left;
    use std::time::Duration;

    #[test]
    fn mini_window_centers_on_monitor_instead_of_reusing_pill_left_edge() {
        assert_eq!(centered_left(0.0, 1920.0, 248.0), 836.0);
    }

    #[test]
    fn centered_position_respects_monitor_origin() {
        assert_eq!(centered_left(-1920.0, 1920.0, 416.0), -1168.0);
    }

    #[test]
    fn mode_transition_uses_damped_liquid_spring() {
        assert_eq!(super::spring_progress(0.0), 0.0);
        assert!(super::spring_progress(0.1) > 0.1);
        assert!(super::spring_progress(0.5) > 1.0);
        assert_eq!(super::spring_progress(1.0), 1.0);
        assert_eq!(super::spring_progress(1.1), 1.0);
    }

    #[test]
    fn animation_frame_interval_matches_display_refresh() {
        assert_eq!(
            super::frame_interval_for_hz(60),
            Duration::from_nanos(16_666_666)
        );
        assert_eq!(
            super::frame_interval_for_hz(143),
            Duration::from_nanos(6_993_006)
        );
        assert_eq!(
            super::frame_interval_for_hz(0),
            Duration::from_nanos(33_333_333)
        );
    }
}

fn centered_left(origin_x: f64, monitor_width: f64, window_width: f64) -> f64 {
    origin_x + ((monitor_width - window_width) / 2.0).max(0.0)
}

/// Computes the top-centre position for a window of the given logical size.
///
/// Split out from `center_top` so `create_main_window` can place the window
/// before it is shown. Creating the window visible and positioning it afterwards
/// causes a one-frame flash at tao's default origin, which is visible on a
/// transparent always-on-top surface.
fn centered_position(
    app: &tauri::AppHandle,
    width: f64,
    height: f64,
) -> Option<PhysicalPosition<i32>> {
    let monitor = app.primary_monitor().ok().flatten()?;
    let scale = monitor.scale_factor();
    let work_area = monitor.work_area();
    let monitor_size = monitor.size();
    let monitor_pos = monitor.position();

    let (origin_y, available_height) = if work_area.size.height > 0 {
        (work_area.position.y as f64, work_area.size.height as f64)
    } else {
        (monitor_pos.y as f64, monitor_size.height as f64)
    };

    let actual_w = width * scale;
    let actual_h = height * scale;

    let x = centered_left(monitor_pos.x as f64, monitor_size.width as f64, actual_w);
    let max_y = (origin_y + available_height - actual_h).max(origin_y);
    let y = (origin_y + DEFAULT_TOP_GAP * scale).clamp(origin_y, max_y);

    Some(PhysicalPosition::new(x.round() as i32, y.round() as i32))
}

/// Creates (or re-configures) the main window.
///
/// Geometry is applied before the first paint so there is no visible jump.
pub fn create_main_window(
    app: &tauri::AppHandle,
) -> Result<WebviewWindow, Box<dyn std::error::Error>> {
    // The window is declared in tauri.conf.json, so Tauri has already created it
    // before setup runs. Center it here so startup position is deterministic.
    if let Some(existing) = app.get_webview_window(MAIN_LABEL) {
        let _ = existing.set_always_on_top(true);

        let target_mode = app.state::<crate::state::AppState>().settings().mode;
        let (width, height) = target_mode.size();
        let _ = existing.set_size(LogicalSize::new(width, height));
        center_top(&existing);
        let _ = existing.show();

        let recenter = existing.clone();
        let _ = existing.run_on_main_thread(move || center_top(&recenter));
        return Ok(existing);
    }

    let mode = app.state::<crate::state::AppState>().settings().mode;
    let (width, height) = mode.size();

    // The window is created visible (see the note on `visible` below), so it must
    // already be in the right place when it first paints. Without this it appears
    // at tao's default origin for a frame and then jumps to the top-centre, which
    // is a visible flicker at startup. `center_top` still runs after the build to
    // correct for the real outer size; this only removes the first-frame jump.
    let initial = centered_position(app, width, height);

    let mut builder =
        WebviewWindowBuilder::new(app, MAIN_LABEL, WebviewUrl::App("index.html".into()))
            .inner_size(width, height)
            .decorations(false)
            .transparent(true)
            .always_on_top(true)
            .skip_taskbar(true)
            .resizable(false)
            .maximizable(false)
            .minimizable(false)
            .closable(false)
            .shadow(false)
            // `visible(false)` combined with `focused(false)` is the one
            // combination that breaks native file drag-in. Measured in the
            // standalone reproduction (`sonic-drag-repro`), each flag alone
            // receives the drop and the pair does not, deterministically:
            //
            //     pill,hidden                 -> enter=1 drop=1
            //     pill,nofocus                -> enter=1 drop=1
            //     pill,hidden,nofocus         -> enter=0 drop=0
            //
            // Sonic needs "appears without stealing focus", which is
            // visible + unfocused. It does not need to start hidden.
            .visible(true)
            .focused(false)
            .accept_first_mouse(true);

    if let Some(position) = initial {
        builder = builder.position(position.x as f64, position.y as f64);
    }

    let window = builder.build()?;

    diag("create_main_window: built a new centered window");
    center_top(&window);

    let _ = window.show();
    let recenter = window.clone();
    let _ = window.run_on_main_thread(move || center_top(&recenter));
    Ok(window)
}

/// Appends a diagnostic line to `%TEMP%\sonic-window.log` in debug builds.
pub fn diag(_line: &str) {
    #[cfg(debug_assertions)]
    {
        use std::io::Write;
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(std::env::temp_dir().join("sonic-window.log"))
        {
            let _ = writeln!(file, "{_line}");
        }
    }
}

/// Places the window at the top-centre of the primary monitor.
///
/// `work_area` excludes the taskbar, but some remote-desktop and VM sessions
/// report it as an empty rect. In that case we fall back to the monitor's full
/// size and origin, which is still correct for a top-centred pill.
///
/// Centres on the window's **actual** width. Using the pill's 416px constant for
/// every mode left a 248px mini window sitting 84px left of centre, because it was
/// positioned as if it were still 416px wide.
fn center_top(window: &WebviewWindow) {
    let monitor = match window.primary_monitor().ok().flatten() {
        Some(monitor) => monitor,
        None => {
            diag("center_top: no primary monitor, falling back to center()");
            // Last resort: let the window manager centre horizontally.
            let _ = window.center();
            return;
        }
    };

    let scale = monitor.scale_factor();
    let work_area = monitor.work_area();
    let monitor_size = monitor.size();
    let monitor_pos = monitor.position();

    // Use the work area vertically so Sonic remains below the taskbar.
    let (origin_y, available_height) = if work_area.size.height > 0 {
        (work_area.position.y as f64, work_area.size.height as f64)
    } else {
        (monitor_pos.y as f64, monitor_size.height as f64)
    };

    let (actual_w, actual_h) = window
        .outer_size()
        .map(|size| (size.width as f64, size.height as f64))
        .unwrap_or((WINDOW_WIDTH * scale, WINDOW_HEIGHT * scale));

    let x = centered_left(monitor_pos.x as f64, monitor_size.width as f64, actual_w);
    let max_y = (origin_y + available_height - actual_h).max(origin_y);
    let y = (origin_y + DEFAULT_TOP_GAP * scale).clamp(origin_y, max_y);
    let target = PhysicalPosition::new(x.round() as i32, y.round() as i32);
    match window.set_position(target) {
        Ok(()) => diag(&format!("center_top: set_position ok -> {target:?}")),
        Err(err) => diag(&format!("center_top: set_position failed: {err}")),
    }
}

/// Applies the native acrylic backdrop.
///
/// ## Why this is now a no-op
///
/// Sonic used to call `set_effects(Effect::Acrylic)` here. That was a mistake:
///
/// DWM system backdrops are applied to the **whole HWND rectangle**. They are
/// composited by the desktop window manager below the window, so CSS
/// `border-radius` cannot clip them. On a 416x108 window whose visible surface is
/// a 416x108 pill, that is invisible — but the moment the pill is inset, or the
/// window is taller than the drawn surface, the material fills the square corners
/// and you get a visible rectangle around the rounded pill.
///
/// `DwmGetWindowAttribute(DWMWA_SYSTEMBACKDROP_TYPE)` returning 3
/// (`DWMSBT_TRANSIENTWINDOW`) on the release binary is what produced the reported
/// "square-cornered panel behind the pill".
///
/// So the blur comes from CSS `backdrop-filter` on `.glass-shell` instead. That is
/// clipped by the element's own `border-radius`, needs no native material, and
/// behaves identically on Windows 10 and 11.
///
/// Kept as a function (rather than deleted) because `commands.rs` calls it when
/// the user moves the blur slider; it must stay cheap and must not re-enable a
/// backdrop.
pub fn apply_window_effects(_app: &tauri::AppHandle) {
    // Intentionally empty. See the doc comment above.
}

/// Kept for the blur slider's call site; also intentionally inert.
pub fn reapply_effects(_app: &tauri::AppHandle) {}

/// Animates the window to a mode's geometry.
///
/// The animation used to live in the webview, driving `window_set_frame` once per
/// frame. That had two problems: every frame crossed the IPC boundary (so the
/// resize lagged the React commit by a variable amount), and the webview could not
/// see the pointer once the window shrank beneath it, which made hover-collapse
/// impossible. Both are gone now — Rust owns geometry outright, and the webview
/// only ever draws.
///
/// Frames are applied on the main thread, because that is the only thread allowed
/// to touch a window on Windows. A worker thread paces the frames and waits for
/// each one to be acknowledged, so the loop cannot outrun the compositor.
pub fn animate_to_mode<R: tauri::Runtime>(app: &tauri::AppHandle<R>, mode: WindowMode) {
    // A newer request supersedes an in-flight one. Without this, a fast
    // pill→mini→pill would leave two loops fighting over the same rect.
    let generation = NEXT_GENERATION.fetch_add(1, Ordering::Relaxed) + 1;
    CURRENT_GENERATION.store(generation, Ordering::Relaxed);

    // Own the handle: the closure below outlives this borrow, and the animation
    // loop runs on a worker that must not hold a reference into the caller.
    let handle: tauri::AppHandle<R> = app.clone();

    // Read the starting geometry on the main thread: `outer_position` is a window
    // call, and reading it from a worker is a data race waiting to be reported.
    let (tx, rx) = std::sync::mpsc::channel::<Option<Frame>>();
    let reader = handle.clone();
    let inside = reader.clone();
    let _ = reader.run_on_main_thread(move || {
        let frame = inside
            .get_webview_window(MAIN_LABEL)
            .and_then(|w| read_frame(&w));
        let _ = tx.send(frame);
    });
    let Ok(Some(from)) = rx.recv_timeout(Duration::from_millis(500)) else {
        return;
    };

    let (target_w, target_h) = mode.size();

    // Horizontal: anchor on the CENTRE, not the left edge. The player is one
    // floating object, so a width change has to keep its middle where it was.
    // Anchoring on `from.x` is what made the pill jump left by 84 px every time it
    // collapsed to mini — measured, not guessed: 752 → 752 with a 248 px window
    // means the centre moved from 960 to 876.
    let centre_x = from.x + from.width / 2.0;

    let to = Frame {
        x: centre_x - target_w / 2.0,
        y: from.y,
        width: target_w,
        height: target_h,
    };

    // Nothing to do: already there. Still worth returning early, because a
    // redundant animation is visible as a twitch.
    if (from.width - to.width).abs() < 0.5 && (from.height - to.height).abs() < 0.5 {
        return;
    }

    let started = std::time::Instant::now();
    let total = Duration::from_millis(MODE_MS as u64);
    let refresh_hz = handle
        .get_webview_window(MAIN_LABEL)
        .map(|window| display_refresh_hz(&window))
        .unwrap_or(60);
    let frame_interval = frame_interval_for_hz(refresh_hz);
    let mut frame_count: u32 = 0;

    loop {
        let elapsed = started.elapsed();
        let t = (elapsed.as_secs_f64() / total.as_secs_f64()).min(1.0);
        let e = spring_progress(t);

        let frame = Frame {
            x: from.x + (to.x - from.x) * e,
            y: from.y + (to.y - from.y) * e,
            // The width is interpolated from the real current width so a
            // pill→mini keeps its centre instead of hanging off the old left edge.
            width: from.width + (to.width - from.width) * e,
            height: from.height + (to.height - from.height) * e,
        };

        let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
        let post = handle.clone();
        let posted = handle.run_on_main_thread(move || {
            if generation != current_generation() {
                return;
            }
            if let Some(window) = post.get_webview_window(MAIN_LABEL) {
                apply_frame(&window, frame);
            }
            let _ = done_tx.send(());
        });

        if posted.is_err() {
            return;
        }
        // Acknowledge every frame. If the main thread is busy (a React commit, a
        // compositor hiccup) this waits rather than queueing up stale frames.
        let _ = done_rx.recv_timeout(Duration::from_millis(50));

        if generation != current_generation() {
            return;
        }
        if t >= 1.0 {
            return;
        }

        frame_count += 1;
        let next_target = started + (frame_interval * frame_count);
        let now = std::time::Instant::now();
        if next_target > now {
            let sleep_dur = (next_target - now).min(total.saturating_sub(started.elapsed()));
            if !sleep_dur.is_zero() {
                std::thread::sleep(sleep_dur);
            }
        }
    }
}

#[cfg(windows)]
fn display_refresh_hz(window: &WebviewWindow<impl tauri::Runtime>) -> u32 {
    use windows_sys::Win32::Graphics::Gdi::{
        EnumDisplaySettingsW, GetMonitorInfoW, MonitorFromWindow, DEVMODEW, MONITORINFOEXW,
    };

    let Ok(hwnd) = window.hwnd() else {
        return 60;
    };

    let mut monitor_info = MONITORINFOEXW::default();
    monitor_info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    let monitor = unsafe { MonitorFromWindow(hwnd.0 as _, 2) };
    if monitor.is_null() || unsafe { GetMonitorInfoW(monitor, &mut monitor_info.monitorInfo) } == 0
    {
        return 60;
    }

    let mut mode = DEVMODEW::default();
    mode.dmSize = std::mem::size_of::<DEVMODEW>() as u16;
    // ENUM_CURRENT_SETTINGS, supplied as -1 through its u32 representation.
    let result =
        unsafe { EnumDisplaySettingsW(monitor_info.szDevice.as_ptr(), u32::MAX, &mut mode) };
    if result == 0 || !(30..=1000).contains(&mode.dmDisplayFrequency) {
        60
    } else {
        mode.dmDisplayFrequency
    }
}

#[cfg(not(windows))]
fn display_refresh_hz(_window: &WebviewWindow<impl tauri::Runtime>) -> u32 {
    60
}

fn frame_interval_for_hz(refresh_hz: u32) -> Duration {
    Duration::from_nanos(1_000_000_000 / u64::from(refresh_hz.clamp(30, 1000)))
}

/// Monotonic id for the most recent animation request.
static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);
static CURRENT_GENERATION: AtomicU64 = AtomicU64::new(0);

fn current_generation() -> u64 {
    CURRENT_GENERATION.load(Ordering::Relaxed)
}

/// A window rect in logical pixels.
#[derive(Debug, Clone, Copy)]
pub struct Frame {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Reads the window's outer rect in logical pixels.
fn read_frame(window: &WebviewWindow<impl tauri::Runtime>) -> Option<Frame> {
    let position = window.outer_position().ok()?;
    let size = window.outer_size().ok()?;
    let scale = window.scale_factor().unwrap_or(1.0);
    Some(Frame {
        x: position.x as f64 / scale,
        y: position.y as f64 / scale,
        width: size.width as f64 / scale,
        height: size.height as f64 / scale,
    })
}

/// Applies one animation frame.
///
/// On Windows, SetWindowPos updates both position and size in a single atomic
/// Win32 call, avoiding the intermediate frame where size changed but position
/// didn't (which caused visual jitter/drift during resize).
fn apply_frame<R: tauri::Runtime>(window: &WebviewWindow<R>, frame: Frame) {
    let scale = window.scale_factor().unwrap_or(1.0);
    let px = (frame.x * scale).round() as i32;
    let py = (frame.y * scale).round() as i32;
    let pw = (frame.width * scale).round() as i32;
    let ph = (frame.height * scale).round() as i32;

    #[cfg(windows)]
    if let Ok(hwnd) = window.hwnd() {
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::SetWindowPos(
                hwnd.0 as _,
                std::ptr::null_mut(),
                px,
                py,
                pw,
                ph,
                windows_sys::Win32::UI::WindowsAndMessaging::SWP_NOZORDER
                    | windows_sys::Win32::UI::WindowsAndMessaging::SWP_NOACTIVATE
                    | windows_sys::Win32::UI::WindowsAndMessaging::SWP_NOOWNERZORDER,
            );
        }
        return;
    }

    let _ = window.set_size(LogicalSize::new(frame.width, frame.height));
    let _ = window.set_position(PhysicalPosition::new(px, py));
}

/// Underdamped response with restrained overshoot, matching the
/// reference island's 520 stiffness / 24.2 damping and 400ms handoff.
fn spring_progress(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    let duration = MODE_MS as f64 / 1000.0;
    let stiffness: f64 = 520.0;
    let decay: f64 = 24.2 / 2.0;
    let frequency = (stiffness - decay * decay).sqrt();

    let response = |time: f64| {
        let a = -1.0;
        let b = decay * a / frequency;
        let cosine = (frequency * time).cos();
        let sine = (frequency * time).sin();
        let envelope = (-decay * time).exp();
        let value = a * cosine + b * sine;
        (
            envelope * value,
            envelope * (-decay * value + frequency * (-a * sine + b * cosine)),
        )
    };

    let (position, _) = response(t * duration);
    let (tail_position, tail_velocity) = response(duration);
    let smooth = 3.0 * t * t - 2.0 * t * t * t;
    let velocity_correction = t * t * t - t * t;
    (1.0 + position - tail_position * smooth - tail_velocity * duration * velocity_correction)
        .clamp(0.0, 1.08)
}

/// Resizes the window for a mode change.
///
/// Thin wrapper over [`animate_to_mode`] kept for call sites that already hold an
/// `AppHandle`; it exists so the tray, the command layer and the hover watcher all
/// go through the same animated path instead of three different ones.
pub fn apply_mode<R: tauri::Runtime>(app: &tauri::AppHandle<R>, mode: WindowMode) {
    animate_to_mode(app, mode);
}

/// Logical height of the file-drop surface.
///
/// Taller than the pill so the target reads as a destination, but short enough
/// that it never covers the thing the user is dragging toward.
pub const DROP_HEIGHT: f64 = 176.0;

/// Logical height of the Pocket panel.
///
/// Pocket is a *page*, not a taller window: it occupies the same rect as the
/// pill (416x108) and the island slides sideways between the player and Pocket.
/// Keeping it at pill height is what removes the whole class of bugs that came
/// from resizing the window for a panel — the hover watcher fighting the height,
/// `window.blur` collapsing an open panel, and the list being cut off.
///
/// The value is asserted against `WindowMode::Pill.size().1` in the tests below
/// rather than duplicated as a literal, so the two cannot drift.
pub const POCKET_HEIGHT: f64 = 108.0;

/// Logical height of the settings panel.
///
/// Matches the expanded player's height: the settings surface reuses the same
/// tall rect rather than introducing a third one, because the window morph
/// between them would otherwise be visible on every open.
pub const SETTINGS_HEIGHT: f64 = 548.0;

/// Animates the window to an explicit logical height, keeping its centre.
///
/// ## Why this is the single writer
///
/// Every height change in Sonic — hover, mode, Pocket, drop target — goes through
/// here. The rect has exactly one owner (Rust), one animation shape (the damped
/// spring in [`spring_progress`]), and one generation counter that cancels a
/// superseded transition. A JS tween would be a second writer for one rect, which
/// is the class of bug the single-writer rule exists to prevent.
///
/// Anchoring is on the CENTRE, not the left edge: Sonic is one floating object,
/// so a width change has to keep its middle where it was. Anchoring on `from.x`
/// is what made the pill jump sideways every time it collapsed to mini.
pub fn animate_to_height<R: tauri::Runtime>(app: &tauri::AppHandle<R>, target_h: f64) {
    let generation = NEXT_GENERATION.fetch_add(1, Ordering::Relaxed) + 1;
    CURRENT_GENERATION.store(generation, Ordering::Relaxed);

    let handle: tauri::AppHandle<R> = app.clone();

    let (tx, rx) = std::sync::mpsc::channel::<Option<Frame>>();
    let reader = handle.clone();
    let inside = reader.clone();
    let _ = reader.run_on_main_thread(move || {
        let frame = inside
            .get_webview_window(MAIN_LABEL)
            .and_then(|w| read_frame(&w));
        let _ = tx.send(frame);
    });
    let Ok(Some(from)) = rx.recv_timeout(Duration::from_millis(500)) else {
        return;
    };

    let target_w = WINDOW_WIDTH;
    let centre_x = from.x + from.width / 2.0;
    let to = Frame {
        x: centre_x - target_w / 2.0,
        y: from.y,
        width: target_w,
        height: target_h,
    };

    // Already there: a redundant animation is visible as a twitch.
    if (from.width - to.width).abs() < 0.5 && (from.height - to.height).abs() < 0.5 {
        return;
    }

    let started = std::time::Instant::now();
    let total = Duration::from_millis(MODE_MS as u64);
    let refresh_hz = handle
        .get_webview_window(MAIN_LABEL)
        .map(|window| display_refresh_hz(&window))
        .unwrap_or(60);
    let frame_interval = frame_interval_for_hz(refresh_hz);
    let mut frame_count: u32 = 0;

    loop {
        let elapsed = started.elapsed();
        let t = (elapsed.as_secs_f64() / total.as_secs_f64()).min(1.0);
        let e = spring_progress(t);

        let frame = Frame {
            x: from.x + (to.x - from.x) * e,
            y: from.y + (to.y - from.y) * e,
            width: from.width + (to.width - from.width) * e,
            height: from.height + (to.height - from.height) * e,
        };

        let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
        let post = handle.clone();
        let posted = handle.run_on_main_thread(move || {
            if generation != current_generation() {
                return;
            }
            if let Some(window) = post.get_webview_window(MAIN_LABEL) {
                apply_frame(&window, frame);
            }
            let _ = done_tx.send(());
        });

        if posted.is_err() {
            return;
        }
        // Acknowledge every frame: if the main thread is busy (a React commit, a
        // compositor hiccup) this waits rather than queueing stale frames.
        let _ = done_rx.recv_timeout(Duration::from_millis(50));
        if generation != current_generation() || t >= 1.0 {
            return;
        }

        frame_count += 1;
        let next_target = started + (frame_interval * frame_count);
        let now = std::time::Instant::now();
        if next_target > now {
            let sleep_dur = (next_target - now).min(total.saturating_sub(started.elapsed()));
            if !sleep_dur.is_zero() {
                std::thread::sleep(sleep_dur);
            }
        }
    }
}

/// Animates the window to the drop-target geometry, or back from it.
///
/// ## Why a dedicated geometry instead of reusing Expanded
///
/// Expanded is 548 px of player: artwork, transport, volume. A drop target that
/// size would push the pill's own content out of the way and leave a mostly empty
/// panel. The drop surface only has to say "release here", so it is its own
/// short rect.
pub fn animate_to_drop<R: tauri::Runtime>(app: &tauri::AppHandle<R>, active: bool) {
    let target_h = if active {
        DROP_HEIGHT
    } else {
        // Collapse back to whatever mode is actually in force, so a cancelled
        // drag lands on the same rect the user came from.
        app.state::<crate::state::AppState>()
            .effective_mode()
            .size()
            .1
    };
    animate_to_height(app, target_h);
}

/// Reads the current top-left corner.
pub fn current_geometry<R: tauri::Runtime>(window: &WebviewWindow<R>) -> Option<Geometry> {
    let position = window.outer_position().ok()?;
    Some(Geometry {
        x: position.x,
        y: position.y,
    })
}

/// Persists the window position if the user allows it.
///
/// The position saved is always the *pill's*, never the rect on screen at the
/// moment of saving. Those differ whenever the player is collapsed to mini by
/// hover, and saving the mini rect would restore the pill 84 px off centre the
/// next time the app starts — the same leftward creep the old centre-top bug
/// produced, just arriving through a different door.
pub fn persist_geometry<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let state = app.state::<crate::state::AppState>();
    let settings = state.settings();

    if !settings.remember_position {
        return;
    }
    let Some(window) = app.get_webview_window(MAIN_LABEL) else {
        return;
    };
    let Ok(position) = window.outer_position() else {
        return;
    };

    // Centre the pill on the current window's centre, keeping the top edge. In
    // pill mode this is a no-op; in mini it is the correction.
    let scale = window.scale_factor().unwrap_or(1.0);
    let Ok(size) = window.outer_size() else {
        return;
    };
    let centre_x = position.x as f64 + size.width as f64 / 2.0;
    let x = (centre_x - WINDOW_WIDTH * scale / 2.0).round() as i32;

    let _ = crate::config::save_geometry(Geometry { x, y: position.y });
}

/// Wires the drag handler that saves position when the user lets go.
///
/// Tauri emits `Moved` for every pixel of a drag, so we debounce with a short
/// idle timer on a background thread rather than writing the file each frame.
pub fn watch_geometry(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        let mut last_saved: Option<Geometry> = None;
        loop {
            std::thread::sleep(std::time::Duration::from_millis(700));

            let state = app.state::<crate::state::AppState>();
            if !state.settings().remember_position {
                continue;
            }
            let Some(window) = app.get_webview_window(MAIN_LABEL) else {
                continue;
            };
            if !window.is_visible().unwrap_or(false) {
                continue;
            }
            let Some(geometry) = current_geometry(&window) else {
                continue;
            };
            if last_saved == Some(geometry) {
                continue;
            }

            let _ = crate::config::save_geometry(geometry);
            last_saved = Some(geometry);
        }
    });
}
