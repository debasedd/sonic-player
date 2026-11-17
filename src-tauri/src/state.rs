//! Shared application state.
//!
//! One `AppState` is managed by Tauri. Everything the UI is allowed to know
//! lives behind [`UiState`]; credentials never do.

use crate::config::{self, Geometry, Settings, WindowMode};
use crate::error::AppError;
use crate::spotify::{Device, Item, Playback, SpotifyClient};
use std::sync::{Arc, Mutex, RwLock};

/// Snapshot handed to the frontend on connect and after every poll.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UiState {
    /// True once a refresh token exists.
    pub connected: bool,
    /// True while a login round-trip is in flight.
    pub authenticating: bool,
    /// Human-readable account label, e.g. "m83 · Premium".
    pub account_label: String,
    pub account_name: String,
    pub avatar_url: String,
    pub is_premium: bool,
    /// Present when the last Spotify call failed.
    pub error: Option<crate::error::AppError>,
    /// Devices Spotify currently reports.
    pub devices: Vec<Device>,
    /// Current playback, if any.
    pub playback: Option<Playback>,
    /// Settings as the UI understands them.
    pub settings: UiSettings,
    /// True until the first successful playback fetch.
    pub ready: bool,
    /// Whether Windows currently exposes a local media session.
    pub local_media_available: bool,
    /// Local Windows media session play state.
    pub local_media_playing: bool,
    /// Local Windows media session title, artist, and embedded cover art.
    pub local_media_title: Option<String>,
    pub local_media_artist: Option<String>,
    pub local_media_artwork_data_url: Option<String>,
    /// Local timeline position and duration in milliseconds, when published.
    pub local_media_position_ms: Option<u64>,
    pub local_media_duration_ms: Option<u64>,
    /// Local shuffle/repeat state, when the session reports them.
    pub local_media_shuffle: Option<bool>,
    pub local_media_repeat: Option<String>,
    /// Which transport controls the local session accepts right now.
    #[serde(default)]
    pub local_media_controls: crate::local_media::LocalControls,
    /// True while the pointer is resting on the player and it is expanded to pill.
    pub hover_expand: bool,
    pub hover_mini: bool,
    /// The mode the UI should actually draw. `mini` + `hover_expand` = `pill`.
    ///
    /// The frontend must render this, not `settings.mode`, or the window and the
    /// DOM disagree for the length of a hover.
    pub effective_mode: WindowMode,
}

/// Settings subset the UI needs (everything except the client id, which the
/// frontend never has to render).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UiSettings {
    /// When true, resting shape is mini; hover expands it to pill.
    pub hover_expand: bool,
    pub hover_mini: bool,
    pub transparency: u8,
    pub blur: u8,
    /// Flat opaque surface instead of Liquid Glass. Mirrors `Settings::solid_surface`.
    pub solid_surface: bool,
    pub mode: WindowMode,
    pub always_on_top: bool,
    pub start_with_windows: bool,
    pub remember_position: bool,
    pub show_tray: bool,
    pub volume: u8,
    pub has_client_id: bool,
    /// Redirect URI the user must allow-list in the Spotify dashboard.
    pub redirect_uri: String,

    // -- Pocket -------------------------------------------------------------
    pub shelf_enabled: bool,
    pub shelf_max_mib: u32,
    pub shelf_count: u32,
    pub shelf_bytes: u64,
    pub clipboard_enabled: bool,
    pub clipboard_private: bool,
    pub clipboard_max_entries: u32,
    pub clipboard_retention_days: u32,
    pub clipboard_excluded_apps: Vec<String>,
    pub clipboard_count: u32,
    pub clipboard_bytes: u64,
    /// Bumped on every capture so the UI can refresh the vault list.
    pub clipboard_revision: u64,
    /// Live clipboard sequence number, for the "monitoring" indicator.
    pub clipboard_sequence: u32,
    /// True when the window is currently showing a file drop target.
    pub drop_target: bool,
}

/// Pocket counters, cached in memory.
///
/// Kept out of `Settings` (which is user preferences) and out of the JSON read
/// path: `ui_state()` runs on every poll and every command, and re-reading a
/// clipboard index that can hold several megabytes of captured text once a
/// second would be a self-inflicted performance bug. The index files remain the
/// source of truth; these numbers are refreshed whenever the files change.
#[derive(Debug, Clone, Copy, Default)]
pub struct PocketStats {
    pub shelf_count: u32,
    pub shelf_bytes: u64,
    pub clipboard_count: u32,
    pub clipboard_bytes: u64,
}

impl UiSettings {
    /// Settings-only snapshot.
    ///
    /// Used by `get_settings` and `update_settings`, which return the settings
    /// the frontend just changed. Pocket counters are attached with
    /// [`UiSettings::with_pocket`] when the full snapshot is needed.
    pub fn from_settings(settings: &Settings) -> Self {
        Self {
            hover_expand: settings.hover_expand,
            hover_mini: settings.hover_expand,
            transparency: settings.transparency,
            blur: settings.blur,
            solid_surface: settings.solid_surface,
            mode: settings.mode,
            always_on_top: settings.always_on_top,
            start_with_windows: settings.start_with_windows,
            remember_position: settings.remember_position,
            show_tray: settings.show_tray,
            volume: settings.volume,
            has_client_id: !settings.client_id.trim().is_empty(),
            redirect_uri: config::redirect_uri(settings.redirect_port),
            shelf_enabled: settings.shelf_enabled,
            shelf_max_mib: settings.shelf_max_mib,
            shelf_count: 0,
            shelf_bytes: 0,
            clipboard_enabled: settings.clipboard_enabled,
            clipboard_private: settings.clipboard_private,
            clipboard_max_entries: settings.clipboard_max_entries,
            clipboard_retention_days: settings.clipboard_retention_days,
            clipboard_excluded_apps: settings.clipboard_excluded_apps.clone(),
            clipboard_count: 0,
            clipboard_bytes: 0,
            clipboard_revision: 0,
            clipboard_sequence: 0,
            drop_target: false,
        }
    }

    /// Attaches Pocket counters and live clipboard state.
    pub fn with_pocket(
        mut self,
        pocket: PocketStats,
        clipboard_revision: u64,
        clipboard_sequence: u32,
        drop_target: bool,
    ) -> Self {
        self.shelf_count = pocket.shelf_count;
        self.shelf_bytes = pocket.shelf_bytes;
        self.clipboard_count = pocket.clipboard_count;
        self.clipboard_bytes = pocket.clipboard_bytes;
        self.clipboard_revision = clipboard_revision;
        self.clipboard_sequence = clipboard_sequence;
        self.drop_target = drop_target;
        self
    }
}

/// Cached playback plus the device list, written by the poller and by commands.
#[derive(Default, Clone)]
pub struct CachedPlayback {
    pub playback: Option<Playback>,
    pub devices: Vec<Device>,
}

/// Everything the backend shares.
#[derive(Clone)]
pub struct AppState {
    inner: Arc<Inner>,
}

struct Inner {
    settings: RwLock<Settings>,
    client: Mutex<Option<SpotifyClient>>,
    /// Set while a login is pending so it can be cancelled.
    pending_login: Mutex<Option<std::sync::mpsc::Sender<()>>>,
    authenticating: Mutex<bool>,
    cache: Mutex<CachedPlayback>,
    /// Last error surfaced to the UI.
    last_error: Mutex<Option<AppError>>,
    ready: Mutex<bool>,
    local_media: Mutex<crate::local_media::LocalPlaybackState>,
    account: Mutex<AccountInfo>,
    /// Transient pointer-hover state. Never persisted, never survives a restart.
    hover_expand: Mutex<bool>,
    /// True while a drag or a popover owns the pointer. The hover watcher stands
    /// down so it cannot shrink the window out from under the user's hand.
    pointer_claimed: Mutex<bool>,
    /// True while the settings panel is open. Hover stands down for the same
    /// reason: the window is 548 px tall, so a hit test against the 108 px pill
    /// rect would collapse it while the user is reading.
    settings_open: Mutex<bool>,
    /// True while the Pocket panel is open.
    ///
    /// Tracked separately from `settings_open` because the two panels have
    /// different heights, and because a cancelled file drag has to restore the
    /// rect of whichever one is actually on screen. Without this the window
    /// collapsed to pill height behind an open Pocket, which clipped the list.
    pocket_open: Mutex<bool>,
    /// Bumped when the Clipboard Vault content changes, so the webview can
    /// re-read the index without a full state snapshot.
    clipboard_revision: Mutex<u64>,
    /// Live clipboard sequence number, for the "monitoring" indicator.
    clipboard_sequence: Mutex<u32>,
    /// True while an external file drag is over the player.
    ///
    /// Kept in Rust rather than the webview because the drag source is the OS:
    /// the webview's DOM events cannot represent a native Explorer drag reliably,
    /// and the hover watcher has to know a drag owns the pointer so it cannot
    /// collapse the window underneath the drop target.
    drop_target: Mutex<bool>,
    /// Cached Pocket counters, refreshed whenever the index files change.
    pocket_stats: RwLock<PocketStats>,
}

#[derive(Default)]
struct AccountInfo {
    name: String,
    avatar_url: String,
    is_premium: bool,
}

impl AppState {
    /// Loads persisted settings and rebuilds the client if a token exists.
    pub fn new() -> Self {
        let settings = config::load_settings();
        let stored = crate::vault::load().unwrap_or_default();

        let client = if stored.is_empty() {
            None
        } else {
            Some(SpotifyClient::new(settings.clone(), stored.refresh_token))
        };

        let state = Self {
            inner: Arc::new(Inner {
                settings: RwLock::new(settings),
                client: Mutex::new(client),
                pending_login: Mutex::new(None),
                authenticating: Mutex::new(false),
                cache: Mutex::new(CachedPlayback::default()),
                last_error: Mutex::new(None),
                ready: Mutex::new(false),
                local_media: Mutex::new(crate::local_media::LocalPlaybackState::default()),
                hover_expand: Mutex::new(false),
                pointer_claimed: Mutex::new(false),
                settings_open: Mutex::new(false),
                pocket_open: Mutex::new(false),
                clipboard_revision: Mutex::new(0),
                clipboard_sequence: Mutex::new(0),
                drop_target: Mutex::new(false),
                pocket_stats: RwLock::new(PocketStats::default()),
                account: Mutex::new(AccountInfo {
                    name: stored.display_name,
                    avatar_url: stored.avatar_url,
                    is_premium: stored.product == "premium",
                }),
            }),
        };

        // Make sure autostart reflects the persisted preference on every launch.
        let autostart = state.settings().start_with_windows;
        let _ = config::set_autostart(autostart);

        state
    }

    // -- settings ------------------------------------------------------------

    pub fn settings(&self) -> Settings {
        self.inner
            .settings
            .read()
            .expect("settings poisoned")
            .clone()
    }

    pub fn update_settings(&self, mutate: impl FnOnce(&mut Settings)) -> Settings {
        let mut guard = self.inner.settings.write().expect("settings poisoned");
        mutate(&mut guard);
        let snapshot = guard.clone();
        drop(guard);

        // Persist first: a failure here should not leave memory and disk disagreeing.
        if let Err(err) = config::save_settings(&snapshot) {
            self.set_error(AppError::Local(format!("Could not save settings: {err}")));
        }

        if let Some(client) = self.inner.client.lock().expect("client poisoned").as_ref() {
            client.update_settings(snapshot.clone());
        }

        snapshot
    }

    // -- client --------------------------------------------------------------

    /// Returns the client, creating it if settings now have a client id.
    pub fn client(&self) -> Option<SpotifyClient> {
        self.inner.client.lock().expect("client poisoned").clone()
    }

    /// Builds a client from the current settings and stored refresh token.
    fn rebuild_client(&self) -> Option<SpotifyClient> {
        let settings = self.settings();
        if settings.client_id.trim().is_empty() {
            return None;
        }
        let stored = crate::vault::load().unwrap_or_default();
        if stored.is_empty() {
            return None;
        }
        let client = SpotifyClient::new(settings, stored.refresh_token);
        *self.inner.client.lock().expect("client poisoned") = Some(client.clone());
        Some(client)
    }

    /// Installs tokens after a successful login.
    pub fn install_tokens(&self, tokens: crate::spotify::TokenSet) -> crate::error::AppResult<()> {
        let settings = self.settings();
        let refresh = tokens
            .refresh_token
            .clone()
            .ok_or_else(|| AppError::Local("Spotify did not return a refresh token.".into()))?;

        let client = SpotifyClient::new(settings, refresh.clone());
        *self.inner.client.lock().expect("client poisoned") = Some(client);

        // Store the refresh token now; profile details are filled in on first poll.
        let mut stored = crate::vault::load().unwrap_or_default();
        stored.refresh_token = refresh;
        stored.obtained_at = crate::spotify::now_epoch();
        crate::vault::save(&stored)?;

        *self.inner.authenticating.lock().expect("auth poisoned") = false;
        Ok(())
    }

    /// Drops all credentials.
    pub fn disconnect(&self) -> crate::error::AppResult<()> {
        crate::vault::clear()?;
        if let Some(client) = self.client() {
            client.clear();
        }
        *self.inner.client.lock().expect("client poisoned") = None;
        *self.inner.cache.lock().expect("cache poisoned") = CachedPlayback::default();
        *self.inner.ready.lock().expect("ready poisoned") = false;
        *self.inner.account.lock().expect("account poisoned") = AccountInfo::default();
        *self.inner.last_error.lock().expect("error poisoned") = None;
        Ok(())
    }

    // -- auth state ----------------------------------------------------------

    pub fn set_authenticating(&self, value: bool) {
        *self.inner.authenticating.lock().expect("auth poisoned") = value;
    }

    pub fn is_authenticating(&self) -> bool {
        *self.inner.authenticating.lock().expect("auth poisoned")
    }

    pub fn set_pending_login(&self, cancel: Option<std::sync::mpsc::Sender<()>>) {
        *self.inner.pending_login.lock().expect("login poisoned") = cancel;
    }

    pub fn cancel_pending_login(&self) {
        let pending = self
            .inner
            .pending_login
            .lock()
            .expect("login poisoned")
            .take();
        if let Some(cancel) = pending {
            let _ = cancel.send(());
        }
        self.set_authenticating(false);
    }

    // -- cache ---------------------------------------------------------------

    pub fn cache(&self) -> CachedPlayback {
        self.inner.cache.lock().expect("cache poisoned").clone()
    }

    pub fn set_cache(&self, playback: Option<Playback>, devices: Vec<Device>) {
        *self.inner.cache.lock().expect("cache poisoned") = CachedPlayback { playback, devices };
    }

    pub fn set_playback(&self, playback: Option<Playback>) {
        self.inner.cache.lock().expect("cache poisoned").playback = playback;
    }

    pub fn set_devices(&self, devices: Vec<Device>) {
        self.inner.cache.lock().expect("cache poisoned").devices = devices;
    }

    // -- errors / readiness --------------------------------------------------

    pub fn set_error(&self, error: AppError) {
        *self.inner.last_error.lock().expect("error poisoned") = Some(error);
    }

    pub fn clear_error(&self) {
        *self.inner.last_error.lock().expect("error poisoned") = None;
    }

    pub fn last_error(&self) -> Option<AppError> {
        self.inner
            .last_error
            .lock()
            .expect("error poisoned")
            .clone()
    }

    pub fn set_ready(&self, ready: bool) {
        *self.inner.ready.lock().expect("ready poisoned") = ready;
    }

    pub fn is_ready(&self) -> bool {
        *self.inner.ready.lock().expect("ready poisoned")
    }

    pub fn local_media(&self) -> crate::local_media::LocalPlaybackState {
        self.inner
            .local_media
            .lock()
            .expect("local media poisoned")
            .clone()
    }

    pub fn set_local_media(&self, local_media: crate::local_media::LocalPlaybackState) {
        *self.inner.local_media.lock().expect("local media poisoned") = local_media;
    }

    // -- account -------------------------------------------------------------

    pub fn set_account(&self, name: String, avatar_url: String, is_premium: bool) {
        let mut account = self.inner.account.lock().expect("account poisoned");
        account.name = name;
        account.avatar_url = avatar_url;
        account.is_premium = is_premium;
    }

    pub fn account(&self) -> (String, String, bool) {
        let account = self.inner.account.lock().expect("account poisoned");
        (
            account.name.clone(),
            account.avatar_url.clone(),
            account.is_premium,
        )
    }

    // -- geometry ------------------------------------------------------------

    /// Persisted geometry, if the user allowed remembering position.
    pub fn saved_geometry(&self) -> Option<Geometry> {
        if !self.settings().remember_position {
            return None;
        }
        config::load_geometry()
    }

    /// Full snapshot for the frontend.
    pub fn ui_state(&self) -> UiState {
        let settings = self.settings();
        let cache = self.cache();
        let local_media = self.local_media();
        let (name, avatar, is_premium) = self.account();
        let error = self.last_error();
        let pocket = self.pocket_stats();
        let clipboard_revision = self.clipboard_revision();

        UiState {
            connected: self.client().map(|c| c.is_authenticated()).unwrap_or(false),
            authenticating: self.is_authenticating(),
            account_label: if name.is_empty() {
                String::new()
            } else if is_premium {
                format!("{name} · Premium")
            } else {
                format!("{name} · Free")
            },
            account_name: name,
            avatar_url: avatar,
            is_premium,
            error,
            devices: cache.devices,
            playback: cache.playback,
            settings: UiSettings::from_settings(&settings).with_pocket(
                pocket,
                clipboard_revision,
                self.clipboard_sequence(),
                self.drop_target(),
            ),
            ready: self.is_ready(),
            local_media_available: local_media.available,
            local_media_playing: local_media.is_playing,
            local_media_title: local_media.title,
            local_media_artist: local_media.artist,
            local_media_artwork_data_url: local_media.artwork_data_url,
            local_media_position_ms: local_media.position_ms,
            local_media_duration_ms: local_media.duration_ms,
            local_media_shuffle: local_media.shuffle,
            local_media_repeat: local_media.repeat,
            local_media_controls: local_media.controls,
            hover_expand: self.is_hover_expand(),
            hover_mini: self.is_hover_mini(),
            effective_mode: self.effective_mode(),
        }
    }

    // -- hover (mini <-> pill) ------------------------------------------------

    pub fn is_hover_expand(&self) -> bool {
        *self.inner.hover_expand.lock().expect("hover poisoned")
    }

    pub fn set_hover_expand(&self, value: bool) {
        *self.inner.hover_expand.lock().expect("hover poisoned") = value;
    }

    pub fn is_hover_mini(&self) -> bool {
        !self.is_hover_expand()
    }

    pub fn set_hover_mini(&self, value: bool) {
        self.set_hover_expand(!value);
    }

    /// The mode the window should be sized for right now.
    ///
    /// Hover-expansion applies to mini mode. If the user has explicitly picked
    /// pill or expanded, that choice wins — hover must never be able to fight an
    /// explicit mode, or the window would oscillate between two sizes.
    pub fn effective_mode(&self) -> WindowMode {
        // Pocket is a page at the pill's rect. It must report Pill regardless of
        // the saved mode, otherwise opening Pocket from mini would shrink the
        // window to 248x60 and the page would be squeezed into it.
        if self.pocket_open() {
            return WindowMode::Pill;
        }

        let base = self.settings().mode;
        if base == WindowMode::Mini && self.is_hover_expand() {
            WindowMode::Pill
        } else {
            base
        }
    }

    pub fn pointer_claimed(&self) -> bool {
        *self.inner.pointer_claimed.lock().expect("claimed poisoned")
    }

    pub fn set_pointer_claimed(&self, value: bool) {
        *self.inner.pointer_claimed.lock().expect("claimed poisoned") = value;
    }

    pub fn settings_open(&self) -> bool {
        *self.inner.settings_open.lock().expect("settings poisoned")
    }

    pub fn set_settings_open(&self, value: bool) {
        *self.inner.settings_open.lock().expect("settings poisoned") = value;
    }

    pub fn pocket_open(&self) -> bool {
        *self.inner.pocket_open.lock().expect("pocket poisoned")
    }

    pub fn set_pocket_open(&self, value: bool) {
        *self.inner.pocket_open.lock().expect("pocket poisoned") = value;
    }

    /// Height of the panel currently occupying the window, if any.
    ///
    /// One function so the drop-cancel path and `set_pocket_open` cannot drift
    /// apart on which surface is on screen.
    ///
    /// Pocket is deliberately **not** listed: it is a page at pill height, so it
    /// never changes the window's height. Only Settings (which is genuinely
    /// taller than the pill) has a panel height.
    pub fn panel_height(&self) -> Option<f64> {
        if self.settings_open() {
            Some(crate::window::SETTINGS_HEIGHT)
        } else {
            None
        }
    }

    // -- clipboard vault revision --------------------------------------------

    /// Monotonic counter bumped whenever the Clipboard Vault changes.
    ///
    /// The webview uses this to notice new captures without polling the index.
    pub fn clipboard_revision(&self) -> u64 {
        *self
            .inner
            .clipboard_revision
            .lock()
            .expect("revision poisoned")
    }

    pub fn bump_clipboard_revision(&self) {
        let mut guard = self
            .inner
            .clipboard_revision
            .lock()
            .expect("revision poisoned");
        *guard = guard.wrapping_add(1);
    }

    /// Marks the vault as changed from the capture path.
    ///
    /// The capture thread has no `AppHandle`, so it cannot push a snapshot. It
    /// bumps the revision instead, and the UI re-reads the list when the revision
    /// moves. This is deliberately a revision bump and not an emit: a clipboard
    /// history that pushed a full state snapshot on every copy would wake React
    /// for every `Ctrl+C` in the system.
    pub fn set_clipboard_dirty(&self, _dirty: bool) {
        self.bump_clipboard_revision();
        *self
            .inner
            .clipboard_sequence
            .lock()
            .expect("sequence poisoned") = crate::clipboard_sys::sequence_number().unwrap_or(0);
    }

    /// Records the live clipboard sequence number.
    ///
    /// Kept separate from [`AppState::set_clipboard_dirty`] so the watcher can
    /// update the "monitoring" indicator without falsely bumping the revision
    /// (which would make the UI re-read the vault list on every poll).
    pub fn set_clipboard_sequence(&self, sequence: u32) {
        *self
            .inner
            .clipboard_sequence
            .lock()
            .expect("sequence poisoned") = sequence;
    }

    /// Live clipboard sequence number, for the "monitoring" indicator.
    pub fn clipboard_sequence(&self) -> u32 {
        *self
            .inner
            .clipboard_sequence
            .lock()
            .expect("sequence poisoned")
    }

    /// True while a native file drag is over the player.
    pub fn drop_target(&self) -> bool {
        *self.inner.drop_target.lock().expect("drop target poisoned")
    }

    pub fn set_drop_target(&self, value: bool) {
        *self.inner.drop_target.lock().expect("drop target poisoned") = value;
    }

    // -- pocket stats ---------------------------------------------------------

    pub fn pocket_stats(&self) -> PocketStats {
        *self
            .inner
            .pocket_stats
            .read()
            .expect("pocket stats poisoned")
    }

    /// Recomputes the cached counters from the index files and returns them.
    ///
    /// Call after any change to the shelf or the vault. Reading the two index
    /// files is cheap here precisely because this is *not* on the per-poll path.
    pub fn refresh_pocket_stats(&self) -> PocketStats {
        let shelf = crate::shelf::Shelf::load();
        let entries = shelf.entries();
        let clipboard = crate::clipboard::ClipboardVault::load();

        let stats = PocketStats {
            shelf_count: entries.len() as u32,
            shelf_bytes: entries.iter().map(|e| e.size).sum(),
            clipboard_count: clipboard.entries().len() as u32,
            clipboard_bytes: clipboard.total_bytes(),
        };

        *self
            .inner
            .pocket_stats
            .write()
            .expect("pocket stats poisoned") = stats;
        stats
    }

    /// Ensures a client exists, building one from storage if needed.
    pub fn ensure_client(&self) -> Option<SpotifyClient> {
        if self.client().is_none() {
            return self.rebuild_client();
        }
        self.client()
    }

    /// Forces a fresh client build (used after the client id changes).
    pub fn reset_client(&self) -> Option<SpotifyClient> {
        *self.inner.client.lock().expect("client poisoned") = None;
        self.ensure_client()
    }
}

/// Convenience used by the poller to stamp an item for display.
pub fn item_summary(item: &Item) -> String {
    if item.artists.is_empty() {
        item.name.clone()
    } else {
        format!("{} — {}", item.name, item.artists.join(", "))
    }
}
