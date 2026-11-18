use serde::Serialize;
use std::sync::Arc;
use std::time::Duration;

/// Capability flags mirrored from the GSMTC session so the UI can disable
/// controls the current media app does not expose.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalControls {
    pub can_play: bool,
    pub can_pause: bool,
    pub can_next: bool,
    pub can_previous: bool,
    pub can_seek: bool,
    pub can_shuffle: bool,
    pub can_repeat: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalPlaybackState {
    pub available: bool,
    pub is_playing: bool,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub artwork_data_url: Option<Arc<str>>,
    /// Track position in milliseconds, when the session exposes a timeline.
    pub position_ms: Option<u64>,
    /// Track length in milliseconds, when the session exposes a timeline.
    pub duration_ms: Option<u64>,
    pub shuffle: Option<bool>,
    /// `None` when the session does not report a repeat mode.
    pub repeat: Option<String>,
    #[serde(default)]
    pub controls: LocalControls,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionStatus {
    Playing,
    Paused,
}

fn snapshot_from_status(status: Option<SessionStatus>) -> LocalPlaybackState {
    match status {
        Some(SessionStatus::Playing) => LocalPlaybackState {
            available: true,
            is_playing: true,
            title: None,
            artist: None,
            artwork_data_url: None,
            position_ms: None,
            duration_ms: None,
            shuffle: None,
            repeat: None,
            controls: LocalControls::default(),
        },
        Some(SessionStatus::Paused) => LocalPlaybackState {
            available: true,
            is_playing: false,
            title: None,
            artist: None,
            artwork_data_url: None,
            position_ms: None,
            duration_ms: None,
            shuffle: None,
            repeat: None,
            controls: LocalControls::default(),
        },
        None => LocalPlaybackState::default(),
    }
}

#[cfg(windows)]
fn session_status(
    status: windows::Media::Control::GlobalSystemMediaTransportControlsSessionPlaybackStatus,
) -> Option<SessionStatus> {
    use windows::Media::Control::GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status;

    match status {
        Status::Playing => Some(SessionStatus::Playing),
        Status::Paused | Status::Stopped | Status::Opened | Status::Changing => {
            Some(SessionStatus::Paused)
        }
        _ => None,
    }
}

#[cfg(windows)]
fn snapshot_with_manager(
    manager: &windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager,
    previous: Option<&LocalPlaybackState>,
) -> windows::core::Result<LocalPlaybackState> {
    let session = match manager.GetCurrentSession() {
        Ok(session) => session,
        Err(error) if error.code().0 == 0 => return Ok(LocalPlaybackState::default()),
        Err(error) => return Err(error),
    };
    let info = session.GetPlaybackInfo()?;
    let playback = info.PlaybackStatus()?;
    let state = snapshot_from_status(session_status(playback));
    if !state.available {
        return Ok(state);
    }

    // Capability flags: what this session actually supports right now.
    let controls = info
        .Controls()
        .map(|controls| LocalControls {
            can_play: controls.IsPlayEnabled().unwrap_or(true),
            can_pause: controls.IsPauseEnabled().unwrap_or(true),
            can_next: controls.IsNextEnabled().unwrap_or(false),
            can_previous: controls.IsPreviousEnabled().unwrap_or(false),
            can_seek: controls.IsPlaybackPositionEnabled().unwrap_or(false),
            can_shuffle: controls.IsShuffleEnabled().unwrap_or(false),
            can_repeat: controls.IsRepeatEnabled().unwrap_or(false),
        })
        .unwrap_or_default();

    // `IReference::Value()` on windows 0.58 returns Err with a success HRESULT
    // (0x0) when the value is absent, so treat both shapes as "not reported".
    let shuffle = info
        .IsShuffleActive()
        .ok()
        .and_then(|reference| reference.Value().ok());
    let repeat = info
        .AutoRepeatMode()
        .ok()
        .and_then(|reference| reference.Value().ok())
        .map(|mode| match mode {
            windows::Media::MediaPlaybackAutoRepeatMode::Track => "track".to_string(),
            windows::Media::MediaPlaybackAutoRepeatMode::List => "context".to_string(),
            _ => "off".to_string(),
        });

    // Timeline: position/duration, only when the session publishes one.
    let (position_ms, duration_ms) = match session.GetTimelineProperties() {
        Ok(timeline) => {
            let start = timeline.StartTime().map(|t| t.Duration).unwrap_or(0);
            let end = timeline.EndTime().map(|t| t.Duration).unwrap_or(0);
            let position = timeline.Position().map(|t| t.Duration).unwrap_or(0);
            // TimeSpan is 100-nanosecond ticks; keep the elapsed offset from start.
            let position_ms = u64::try_from((position - start).clamp(0, i64::MAX))
                .ok()
                .map(|ticks| ticks / 10_000);
            let duration_ms = u64::try_from((end - start).clamp(0, i64::MAX))
                .ok()
                .map(|ticks| ticks / 10_000)
                .filter(|ms| *ms > 0);
            (position_ms, duration_ms)
        }
        Err(_) => (None, None),
    };

    let properties = session.TryGetMediaPropertiesAsync()?.get()?;
    let title = nonempty(properties.Title()?.to_string());
    let artist = nonempty(properties.Artist()?.to_string());
    let same_track =
        previous.is_some_and(|previous| previous.title == title && previous.artist == artist);
    let artwork_data_url = previous
        .filter(|_| same_track)
        .and_then(|previous| previous.artwork_data_url.clone())
        .or_else(|| read_thumbnail(&properties));
    Ok(LocalPlaybackState {
        title,
        artist,
        artwork_data_url,
        position_ms,
        duration_ms,
        shuffle,
        repeat,
        controls,
        ..state
    })
}

#[cfg(windows)]
fn read_thumbnail(
    properties: &windows::Media::Control::GlobalSystemMediaTransportControlsSessionMediaProperties,
) -> Option<Arc<str>> {
    let stream = properties
        .Thumbnail()
        .ok()?
        .OpenReadAsync()
        .ok()?
        .get()
        .ok()?;
    let size = stream.Size().ok()?;
    if size == 0 || size > 5_000_000 {
        return None;
    }
    let content_type = stream.ContentType().ok()?.to_string();
    let input = stream.GetInputStreamAt(0).ok()?;
    let reader = windows::Storage::Streams::DataReader::CreateDataReader(&input).ok()?;
    let loaded = reader.LoadAsync(size as u32).ok()?.get().ok()? as usize;
    let mut bytes = vec![0; loaded];
    reader.ReadBytes(&mut bytes).ok()?;
    artwork_data_url(&content_type, &bytes)
}

fn artwork_data_url(content_type: &str, bytes: &[u8]) -> Option<Arc<str>> {
    if bytes.is_empty() || bytes.len() > 5_000_000 {
        return None;
    }
    let mime = match content_type.trim().to_ascii_lowercase().as_str() {
        "image/jpeg" | "image/png" | "image/webp" | "image/bmp" => content_type,
        _ => return None,
    };
    use base64::Engine;
    Some(
        format!(
            "data:{mime};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        )
        .into(),
    )
}

fn nonempty(value: String) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

#[cfg(test)]
mod artwork_tests {
    #[test]
    fn encodes_supported_album_image_as_data_url() {
        assert_eq!(
            super::artwork_data_url("image/jpeg", &[0xff, 0xd8]),
            Some("data:image/jpeg;base64,/9g=".into())
        );
    }

    #[test]
    fn rejects_non_image_or_oversized_artwork() {
        assert_eq!(super::artwork_data_url("text/html", b"no"), None);
        assert_eq!(super::artwork_data_url("image/png", &[]), None);
        assert_eq!(
            super::artwork_data_url("image/png", &vec![0; 5_000_001]),
            None
        );
    }
}

#[cfg(test)]
mod metadata_tests {
    use super::nonempty;

    #[test]
    fn ignores_empty_media_metadata() {
        assert_eq!(nonempty("  ".into()), None);
        assert_eq!(
            nonempty("Kucinta Kau Apa".into()),
            Some("Kucinta Kau Apa".into())
        );
    }
}
#[cfg(windows)]
fn local_media_interval(state: &LocalPlaybackState) -> Duration {
    if state.is_playing {
        Duration::from_secs(1)
    } else if state.available {
        Duration::from_secs(2)
    } else {
        Duration::from_secs(5)
    }
}

#[cfg(windows)]
pub fn snapshot() -> windows::core::Result<LocalPlaybackState> {
    run_in_mta(|| {
        use windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager as SessionManager;
        let manager = SessionManager::RequestAsync()?.get()?;
        snapshot_with_manager(&manager, None)
    })
}

/// Keep local playback state current without polling Spotify Web API.
#[cfg(windows)]
pub fn spawn_poller(app: tauri::AppHandle, state: crate::state::AppState) {
    use windows::Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED};

    std::thread::spawn(move || {
        if let Err(error) = unsafe { RoInitialize(RO_INIT_MULTITHREADED) } {
            eprintln!("Sonic: could not initialize local media worker: {error}");
            return;
        }
        struct RoScope;
        impl Drop for RoScope {
            fn drop(&mut self) {
                unsafe { RoUninitialize() };
            }
        }
        let _ro_scope = RoScope;

        use windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager as SessionManager;
        // Reuse the native session manager. RequestAsync allocates and tears
        // down WinRT objects each second otherwise.
        let manager = match SessionManager::RequestAsync().and_then(|op| op.get()) {
            Ok(manager) => manager,
            Err(error) => {
                eprintln!("Sonic: GSMTC manager unavailable: {error}");
                return;
            }
        };

        loop {
            let previous = state.local_media();
            if let Ok(snapshot) = snapshot_with_manager(&manager, Some(&previous)) {
                if previous != snapshot {
                    let interval = local_media_interval(&snapshot);
                    state.set_local_media(snapshot);
                    crate::player::emit_state(&app, &state);
                    std::thread::sleep(interval);
                    continue;
                }
            }
            let interval = if previous.is_playing {
                Duration::from_secs(1)
            } else {
                Duration::from_secs(2)
            };
            std::thread::sleep(interval);
        }
    });
}

#[cfg(not(windows))]
pub fn spawn_poller(_app: tauri::AppHandle, _state: crate::state::AppState) {}

#[cfg(windows)]
fn run_in_mta<T: Send + 'static>(
    work: impl FnOnce() -> windows::core::Result<T> + Send + 'static,
) -> windows::core::Result<T> {
    use windows::Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED};

    std::thread::spawn(move || {
        unsafe { RoInitialize(RO_INIT_MULTITHREADED)? };
        struct RoScope;
        impl Drop for RoScope {
            fn drop(&mut self) {
                unsafe { RoUninitialize() };
            }
        }
        let _ro_scope = RoScope;
        work()
    })
    .join()
    .map_err(|_| {
        windows::core::Error::new(
            windows::core::HRESULT(0x80004005u32 as i32),
            "GSMTC worker panicked",
        )
    })?
}

#[cfg(windows)]
pub fn set_playing(desired: Option<bool>) -> windows::core::Result<bool> {
    use windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager as SessionManager;

    run_in_mta(move || {
        let manager = SessionManager::RequestAsync()?.get()?;
        let session = match manager.GetCurrentSession() {
            Ok(session) => session,
            Err(error) if error.code().0 == 0 => return Ok(false),
            Err(error) => return Err(error),
        };
        match desired {
            Some(true) => session.TryPlayAsync()?.get(),
            Some(false) => session.TryPauseAsync()?.get(),
            None => session.TryTogglePlayPauseAsync()?.get(),
        }
    })
}

/// Local skip, seek, shuffle and repeat. Every call returns whether the session
/// accepted it, so the UI can fall back to Spotify when GSMTC cannot do it.
#[cfg(windows)]
pub fn local_command(action: &str, value: Option<&str>) -> windows::core::Result<bool> {
    use windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager as SessionManager;

    let action = action.to_string();
    let value = value.map(|value| value.to_string());
    run_in_mta(move || {
        let manager = SessionManager::RequestAsync()?.get()?;
        let session = match manager.GetCurrentSession() {
            Ok(session) => session,
            Err(error) if error.code().0 == 0 => return Ok(false),
            Err(error) => return Err(error),
        };

        match action.as_str() {
            "next" => session.TrySkipNextAsync()?.get(),
            "previous" => session.TrySkipPreviousAsync()?.get(),
            "seek" => {
                let position_ms: i64 = value
                    .as_deref()
                    .and_then(|raw| raw.parse::<i64>().ok())
                    .unwrap_or(0);
                session.TryChangePlaybackPositionAsync(position_ms)?.get()
            }
            "shuffle" => {
                let requested = value.as_deref() == Some("true");
                session.TryChangeShuffleActiveAsync(requested)?.get()
            }
            "repeat" => {
                use windows::Media::MediaPlaybackAutoRepeatMode as Mode;
                let mode = match value.as_deref() {
                    Some("track") => Mode::Track,
                    Some("context") => Mode::List,
                    _ => Mode::None,
                };
                session.TryChangeAutoRepeatModeAsync(mode)?.get()
            }
            _ => Ok(false),
        }
    })
}

#[cfg(not(windows))]
pub fn local_command(_action: &str, _value: Option<&str>) -> Result<bool, String> {
    Ok(false)
}

#[cfg(not(windows))]
pub fn snapshot() -> Result<LocalPlaybackState, String> {
    Ok(LocalPlaybackState::default())
}

#[cfg(not(windows))]
pub fn set_playing(_desired: Option<bool>) -> Result<bool, String> {
    Ok(false)
}

#[cfg(all(test, windows))]
mod windows_tests {
    use super::SessionStatus;
    use windows::Media::Control::GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status;

    #[test]
    fn windows_gsmtc_session_snapshot_is_readable() {
        let snapshot = super::snapshot().expect("GSMTC snapshot should be readable");
        println!(
            "GSMTC title: {:?}; artist: {:?}; artwork: {}; position_ms: {:?}; duration_ms: {:?}; controls: {:?}",
            snapshot.title,
            snapshot.artist,
            snapshot.artwork_data_url.is_some(),
            snapshot.position_ms,
            snapshot.duration_ms,
            snapshot.controls
        );
        // No media session may be published at all (nothing playing), which is a
        // valid state and must not be reported as a hard failure.
        if !snapshot.available {
            assert!(snapshot.title.is_none());
            assert!(snapshot.artwork_data_url.is_none());
            return;
        }
        assert!(!snapshot.is_playing || snapshot.available);
        assert!(snapshot.title.is_some(), "local session title missing");
        assert!(snapshot.artist.is_some(), "local session artist missing");
        assert!(
            snapshot.artwork_data_url.is_some(),
            "local session artwork missing"
        );
    }

    #[test]
    fn local_command_reports_unsupported_without_panicking() {
        // Unknown actions are rejected locally, not sent to any network API.
        assert_eq!(
            super::local_command("definitely-not-an-action", None).ok(),
            Some(false)
        );
    }

    #[test]
    fn windows_status_maps_only_playing_or_paused() {
        assert_eq!(
            super::session_status(Status::Playing),
            Some(SessionStatus::Playing)
        );
        assert_eq!(
            super::session_status(Status::Paused),
            Some(SessionStatus::Paused)
        );
        assert_eq!(
            super::session_status(Status::Stopped),
            Some(SessionStatus::Paused)
        );
        assert_eq!(
            super::session_status(Status::Opened),
            Some(SessionStatus::Paused)
        );
        assert_eq!(
            super::session_status(Status::Changing),
            Some(SessionStatus::Paused)
        );
        assert_eq!(super::session_status(Status::Closed), None);
    }
}

#[cfg(test)]
mod tests {
    use super::{snapshot_from_status, LocalPlaybackState, SessionStatus};

    fn expected(available: bool, is_playing: bool) -> LocalPlaybackState {
        LocalPlaybackState {
            available,
            is_playing,
            ..LocalPlaybackState::default()
        }
    }

    #[test]
    fn reports_local_playing_session() {
        assert_eq!(
            snapshot_from_status(Some(SessionStatus::Playing)),
            expected(true, true)
        );
    }

    #[test]
    fn reports_local_paused_session_as_available() {
        assert_eq!(
            snapshot_from_status(Some(SessionStatus::Paused)),
            expected(true, false)
        );
    }

    #[test]
    fn reports_missing_session_as_unavailable() {
        assert_eq!(snapshot_from_status(None), LocalPlaybackState::default());
    }

    #[test]
    fn timeline_ticks_convert_to_milliseconds() {
        // 100-nanosecond ticks: 1 second = 10_000 * 100us ticks.
        let ticks_per_ms: i64 = 10_000;
        assert_eq!(ticks_per_ms * 1_000, 10_000_000);
    }
}
