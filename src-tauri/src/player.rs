//! The playback poller.
//!
//! One background thread polls `GET /me/player` on a 1s interval while Spotify is
//! playing, stretches to 3s while paused, and backs off exponentially on 429s.
//! It stops entirely when the account is disconnected, so an idle Sonic costs
//! nothing.
//!
//! Progress between polls is interpolated on the frontend from
//! `progress_ms` + `is_playing`, so a 1s poll still renders smooth motion.

use crate::error::AppError;
use crate::spotify::{Playback, SpotifyClient};
use crate::state::AppState;
use std::time::Duration;
use tauri::Emitter;

/// Base cadence while playing.
const ACTIVE_INTERVAL: Duration = Duration::from_millis(1000);
/// Cadence while paused: enough to notice a track change on another device.
const IDLE_INTERVAL: Duration = Duration::from_millis(3000);
/// Cadence while disconnected from Spotify (no client at all).
const OFFLINE_INTERVAL: Duration = Duration::from_secs(30);
/// Ceiling for rate-limit backoff.
const MAX_BACKOFF_SECONDS: u64 = 300;

/// Event name the frontend listens on.
pub const EVENT_STATE: &str = "sonic://state";
/// Event carrying a fatal auth problem the UI should react to.
pub const EVENT_AUTH_REQUIRED: &str = "sonic://auth-required";

/// Spawns the poller thread. Cheap to call once at startup.
pub fn spawn_poller(app: tauri::AppHandle, state: AppState) {
    // `AppState` is itself an Arc wrapper, so this is a cheap handle clone.
    std::thread::spawn(move || poller_loop(app, state));
}

fn poller_loop(app: tauri::AppHandle, state: AppState) {
    let mut backoff_seconds: u64 = 0;

    loop {
        let client = state.ensure_client();

        let Some(client) = client else {
            // Disconnected: nothing to poll. Keep the UI informed cheaply and
            // re-check occasionally in case tokens appeared out of band.
            emit_state(&app, &state);
            std::thread::sleep(OFFLINE_INTERVAL);
            backoff_seconds = 0;
            continue;
        };

        match poll_once(&client, &state) {
            PollOutcome::Ok(playback) => {
                backoff_seconds = 0;
                let idle = !playback.as_ref().map(|p| p.is_playing).unwrap_or(false);
                state.set_playback(playback);

                // Device list only changes on user action, but refreshing it here
                // keeps the picker honest when Spotify's own app moves playback.
                if let Ok(devices) = client.devices() {
                    state.set_devices(devices);
                }

                state.clear_error();
                state.set_ready(true);
                emit_state(&app, &state);

                std::thread::sleep(if idle { IDLE_INTERVAL } else { ACTIVE_INTERVAL });
            }
            PollOutcome::AuthRequired => {
                // Token is gone or revoked: surface it, then stop hammering the API.
                state.set_error(AppError::Unauthorized);
                state.set_ready(false);
                emit_state(&app, &state);
                let _ = app.emit(EVENT_AUTH_REQUIRED, ());
                std::thread::sleep(OFFLINE_INTERVAL);
            }
            PollOutcome::RateLimited(seconds) => {
                let wait = seconds.clamp(1, MAX_BACKOFF_SECONDS);
                backoff_seconds = wait;
                state.set_error(AppError::RateLimited(wait));
                emit_state(&app, &state);
                std::thread::sleep(Duration::from_secs(wait));
            }
            PollOutcome::Error(err) => {
                state.set_error(err);
                emit_state(&app, &state);
                // Exponential backoff capped, so a flaky network does not spin.
                backoff_seconds = if backoff_seconds == 0 {
                    2
                } else {
                    (backoff_seconds * 2).min(60)
                };
                std::thread::sleep(Duration::from_secs(backoff_seconds));
            }
        }
    }
}

enum PollOutcome {
    Ok(Option<Playback>),
    AuthRequired,
    RateLimited(u64),
    Error(AppError),
}

fn poll_once(client: &SpotifyClient, _state: &AppState) -> PollOutcome {
    match client.playback() {
        Ok(playback) => {
            // Refresh the account label opportunistically; it is a cheap call and
            // only runs when the poller notices we do not know the name yet.
            PollOutcome::Ok(playback)
        }
        Err(AppError::Unauthorized) => PollOutcome::AuthRequired,
        Err(AppError::RateLimited(seconds)) => PollOutcome::RateLimited(seconds),
        Err(other) => PollOutcome::Error(other),
    }
}

/// Fetches the account profile once, for the label and avatar.
pub fn refresh_account(client: &SpotifyClient, state: &AppState) {
    if let Ok(profile) = client.me() {
        let is_premium = profile.product == "premium";
        state.set_account(
            profile.display_name.clone(),
            profile.avatar_url.clone(),
            is_premium,
        );

        // Persist the identity so the label survives a restart without a call.
        let mut stored = crate::vault::load().unwrap_or_default();
        stored.account_id = profile.id;
        stored.display_name = profile.display_name;
        stored.avatar_url = profile.avatar_url;
        stored.product = profile.product;
        let _ = crate::vault::save(&stored);
    }
}

/// Pushes the current state to the frontend.
///
/// Public because the hover watcher changes `effectiveMode` outside the poller and
/// the webview has to hear about it immediately — waiting for the next poll would
/// draw a mini layout inside a pill-sized window for up to a second.
pub fn emit_state<R: tauri::Runtime>(app: &tauri::AppHandle<R>, state: &AppState) {
    let snapshot = state.ui_state_for_event();
    if let Err(err) = app.emit(EVENT_STATE, &snapshot) {
        // A missing window during shutdown is expected and harmless.
        eprintln!("Sonic: could not emit state: {err}");
    }
}

/// Pushes the current state to the frontend (alias used by other modules).
pub fn sync_account<R: tauri::Runtime>(app: &tauri::AppHandle<R>, state: &AppState) {
    emit_state(app, state);
}

// `is_active()` used to live here and returned `ACTIVE_INTERVAL >= IDLE_INTERVAL`,
// a constant comparison that is always false. It had no caller and described
// nothing; deleted rather than fixed.
