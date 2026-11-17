//! Spotify Web API client.
//!
//! Every request goes through [`SpotifyClient`], which owns the current access
//! token, refreshes it when it expires, and retries once on 401. The poller and
//! the command handlers share one client so a single token refresh is visible to
//! all callers.

use crate::config::Settings;
use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const API_BASE: &str = "https://api.spotify.com/v1";
const TOKEN_ENDPOINT: &str = "https://accounts.spotify.com/api/token";

/// How long before expiry we proactively refresh.
const REFRESH_SKEW_SECONDS: u64 = 30;

/// Access token plus its lifetime.
#[derive(Debug, Clone)]
pub struct AccessToken {
    pub value: String,
    /// Unix epoch seconds at which this token stops being usable.
    pub expires_at: u64,
    /// Scopes Spotify granted, space separated.
    pub scope: String,
}

/// The token endpoint response.
#[derive(Debug, Clone, Deserialize)]
pub struct TokenSet {
    pub access_token: String,
    #[serde(default)]
    pub token_type: String,
    #[serde(default)]
    pub expires_in: u64,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub scope: String,
}

impl TokenSet {
    /// Converts to an access token with an absolute expiry.
    pub fn into_access(self) -> AccessToken {
        let now = now_epoch();
        AccessToken {
            value: self.access_token,
            expires_at: now.saturating_add(self.expires_in.max(60)),
            scope: self.scope,
        }
    }
}

pub fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// A shared HTTP agent. Cheap to clone; owns the connection pool.
pub fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        .user_agent("Sonic/0.1 (Windows; Tauri)")
        .http_status_as_error(false)
        .build()
        .new_agent()
}

/// HTTP status of a response, as a plain integer.
pub fn status_of(response: &ureq::http::Response<ureq::Body>) -> u16 {
    response.status().as_u16()
}

/// Reads a `Retry-After` header, in seconds.
pub fn retry_after(response: &ureq::http::Response<ureq::Body>) -> Option<u64> {
    response
        .headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<u64>().ok())
}

/// Reads a Spotify error message out of a JSON error body.
pub fn spotify_error_message(body: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let error = value.get("error")?;
    if let Some(message) = error.get("message").and_then(|m| m.as_str()) {
        return Some(message.to_string());
    }
    error.as_str().map(|s| s.to_string())
}

/// Shared, refresh-aware Spotify client.
#[derive(Clone)]
pub struct SpotifyClient {
    inner: Arc<Mutex<ClientInner>>,
    agent: ureq::Agent,
}

struct ClientInner {
    settings: Settings,
    access: Option<AccessToken>,
    /// Refresh token, mirrored from the vault so we can rotate it in place.
    refresh_token: String,
    /// Serialises refreshes across threads.
    refreshing: bool,
}

impl SpotifyClient {
    pub fn new(settings: Settings, refresh_token: String) -> Self {
        Self {
            inner: Arc::new(Mutex::new(ClientInner {
                settings,
                access: None,
                refresh_token,
                refreshing: false,
            })),
            agent: agent(),
        }
    }

    pub fn is_authenticated(&self) -> bool {
        let inner = self.inner.lock().expect("client poisoned");
        !inner.refresh_token.is_empty()
    }

    /// Updates the client id / redirect port after a settings change.
    pub fn update_settings(&self, settings: Settings) {
        let mut inner = self.inner.lock().expect("client poisoned");
        inner.settings = settings;
    }

    /// Replaces the refresh token after a fresh login.
    pub fn set_refresh_token(&self, token: String) {
        let mut inner = self.inner.lock().expect("client poisoned");
        inner.refresh_token = token;
        // Force the next request to refresh, since the old access token may
        // belong to the previous account.
        inner.access = None;
    }

    /// Clears all in-memory credentials.
    pub fn clear(&self) {
        let mut inner = self.inner.lock().expect("client poisoned");
        inner.refresh_token.clear();
        inner.access = None;
    }

    /// Returns a valid access token, refreshing if needed.
    pub fn access_token(&self) -> AppResult<AccessToken> {
        // Fast path: still valid.
        {
            let inner = self.inner.lock().expect("client poisoned");
            if let Some(token) = &inner.access {
                if token.expires_at > now_epoch() + REFRESH_SKEW_SECONDS {
                    return Ok(token.clone());
                }
            }
            if inner.refresh_token.is_empty() {
                return Err(AppError::Unauthorized);
            }
        }

        self.refresh_access_token()
    }

    /// Performs the refresh-token grant.
    fn refresh_access_token(&self) -> AppResult<AccessToken> {
        // Serialise refreshes: several pollers/commands can hit this at once.
        loop {
            {
                let mut inner = self.inner.lock().expect("client poisoned");
                if !inner.refreshing {
                    inner.refreshing = true;
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(60));
        }

        let result = self.do_refresh();

        {
            let mut inner = self.inner.lock().expect("client poisoned");
            inner.refreshing = false;
        }

        result
    }

    fn do_refresh(&self) -> AppResult<AccessToken> {
        let (client_id, refresh_token) = {
            let inner = self.inner.lock().expect("client poisoned");
            (
                inner.settings.client_id.clone(),
                inner.refresh_token.clone(),
            )
        };

        if refresh_token.is_empty() {
            return Err(AppError::Unauthorized);
        }

        let form = [
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token.as_str()),
            ("client_id", client_id.as_str()),
        ];

        // Spotify's token endpoint requires `application/x-www-form-urlencoded`
        // in the *body*. Sending these as a query string makes Spotify answer
        // with an HTML error page instead of the JSON API, so every failure
        // surfaced as a generic "HTTP 400".
        let response = self
            .agent
            .post(TOKEN_ENDPOINT)
            .send_form(form)
            .map_err(AppError::from)?;
        let status = status_of(&response);
        let body = response
            .into_body()
            .read_to_string()
            .map_err(|e| AppError::Network(format!("unreadable refresh response: {e}")))?;

        if status == 400 {
            // Spotify returns `invalid_grant` once a refresh token is revoked or
            // expired. The user must sign in again.
            return Err(AppError::RefreshFailed(
                "Spotify revoked this session. Reconnect your account.".into(),
            ));
        }
        if status >= 400 {
            let detail = spotify_error_message(&body).unwrap_or_else(|| format!("HTTP {status}"));
            return Err(AppError::RefreshFailed(detail));
        }

        let tokens: TokenSet = serde_json::from_str(&body)
            .map_err(|e| AppError::Local(format!("unexpected refresh response: {e}")))?;

        // Spotify may omit the refresh token on refresh; keep the old one then.
        let rotated = tokens.refresh_token.clone().unwrap_or(refresh_token);
        let access = tokens.into_access();

        {
            let mut inner = self.inner.lock().expect("client poisoned");
            inner.access = Some(access.clone());
            inner.refresh_token = rotated.clone();
        }

        // Persist the (possibly rotated) refresh token.
        let mut stored = crate::vault::load().unwrap_or_default();
        if stored.refresh_token != rotated {
            stored.refresh_token = rotated;
            let _ = crate::vault::save(&stored);
        }

        Ok(access)
    }

    /// GET returning the raw JSON body, refreshing once on 401.
    fn get_json(&self, path: &str, query: &[(&str, String)]) -> AppResult<serde_json::Value> {
        let token = self.access_token()?;
        let url = format!("{API_BASE}{path}");

        for attempt in 0..2 {
            let mut request = self.agent.get(&url);
            request = request.header("Authorization", format!("Bearer {}", token.value));
            for (key, value) in query {
                request = request.query(*key, value.as_str());
            }

            let response = match request.call() {
                Ok(response) => response,
                Err(ureq::Error::StatusCode(401)) if attempt == 0 => {
                    let refreshed = self.refresh_access_token()?;
                    return self.retry_get_with_token(&url, query, &refreshed.value);
                }
                Err(err) => return Err(AppError::from(err)),
            };

            let status = status_of(&response);
            if status == 401 && attempt == 0 {
                let refreshed = self.refresh_access_token()?;
                return self.retry_get_with_token(&url, query, &refreshed.value);
            }
            if status == 204 {
                return Ok(serde_json::Value::Null);
            }
            if status == 429 {
                let seconds = retry_after(&response).unwrap_or(30);
                return Err(AppError::RateLimited(seconds));
            }

            let body = response
                .into_body()
                .read_to_string()
                .map_err(|e| AppError::Network(format!("unreadable response: {e}")))?;

            if status >= 400 {
                return Err(map_status(status, &body));
            }

            if body.trim().is_empty() {
                return Ok(serde_json::Value::Null);
            }
            return serde_json::from_str(&body)
                .map_err(|e| AppError::Local(format!("could not parse Spotify response: {e}")));
        }

        Err(AppError::Unauthorized)
    }

    fn retry_get_with_token(
        &self,
        url: &str,
        query: &[(&str, String)],
        token: &str,
    ) -> AppResult<serde_json::Value> {
        let mut request = self.agent.get(url);
        request = request.header("Authorization", format!("Bearer {token}"));
        for (key, value) in query {
            request = request.query(*key, value.as_str());
        }
        let response = request.call().map_err(AppError::from)?;
        let status = status_of(&response);
        if status == 429 {
            let seconds = retry_after(&response).unwrap_or(30);
            return Err(AppError::RateLimited(seconds));
        }
        if status >= 400 {
            let body = response.into_body().read_to_string().unwrap_or_default();
            return Err(map_status(status, &body));
        }
        let body = response
            .into_body()
            .read_to_string()
            .map_err(|e| AppError::Network(format!("unreadable response: {e}")))?;
        if body.trim().is_empty() {
            return Ok(serde_json::Value::Null);
        }
        serde_json::from_str(&body)
            .map_err(|e| AppError::Local(format!("could not parse Spotify response: {e}")))
    }

    /// PUT with an empty body; used by every playback control.
    fn put_empty(&self, path: &str, query: &[(&str, String)]) -> AppResult<()> {
        let token = self.access_token()?;
        let url = format!("{API_BASE}{path}");

        for attempt in 0..2 {
            let used_token = if attempt == 0 {
                token.value.clone()
            } else {
                self.refresh_access_token()?.value
            };

            let mut request = self.agent.put(&url);
            request = request.header("Authorization", format!("Bearer {used_token}"));
            for (key, value) in query {
                request = request.query(*key, value.as_str());
            }

            let response = match request.send_empty() {
                Ok(response) => response,
                Err(ureq::Error::StatusCode(401)) => continue,
                Err(err) => return Err(AppError::from(err)),
            };

            let status = status_of(&response);
            if status == 401 && attempt == 0 {
                continue;
            }
            if status == 429 {
                let seconds = retry_after(&response).unwrap_or(30);
                return Err(AppError::RateLimited(seconds));
            }
            if status >= 400 {
                let body = response.into_body().read_to_string().unwrap_or_default();
                return Err(map_status(status, &body));
            }
            return Ok(());
        }

        Err(AppError::Unauthorized)
    }

    // -- Endpoint wrappers ---------------------------------------------------

    /// `GET /me` — account profile.
    pub fn me(&self) -> AppResult<Profile> {
        let value = self.get_json("/me", &[])?;
        Ok(Profile {
            id: string_field(&value, "id"),
            display_name: string_field(&value, "display_name"),
            avatar_url: value
                .pointer("/images/0/url")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            product: string_field(&value, "product"),
        })
    }

    /// `GET /me/player` — current playback state.
    ///
    /// Returns `Ok(None)` when nothing is playing and no device is active.
    pub fn playback(&self) -> AppResult<Option<Playback>> {
        let value = self.get_json("/me/player", &[("additional_types", "track".into())])?;

        if value.is_null() {
            return Ok(None);
        }

        let device = value
            .get("device")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        let item = value
            .get("item")
            .cloned()
            .unwrap_or(serde_json::Value::Null);

        let is_playing = value
            .get("is_playing")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let progress_ms = value
            .get("progress_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        Ok(Some(Playback {
            is_playing,
            progress_ms,
            device: parse_device(&device),
            item: if item.is_null() {
                None
            } else {
                Some(parse_item(&item))
            },
            shuffle_state: value
                .get("shuffle_state")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            repeat_state: string_field(&value, "repeat_state"),
        }))
    }

    /// `GET /me/player/devices`.
    pub fn devices(&self) -> AppResult<Vec<Device>> {
        let value = self.get_json("/me/player/devices", &[])?;
        let list = value
            .get("devices")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        Ok(list.iter().map(parse_device).collect())
    }

    /// `PUT /me/player/play`
    pub fn play(&self, device_id: Option<&str>, uris: Option<Vec<String>>) -> AppResult<()> {
        let mut query: Vec<(&str, String)> = Vec::new();
        if let Some(device) = device_id {
            query.push(("device_id", device.to_string()));
        }
        // A body is only needed to start a specific context; resume needs none.
        if let Some(uris) = uris {
            let body = serde_json::json!({ "uris": uris }).to_string();
            let token = self.access_token()?;
            let mut request = self.agent.put(format!("{API_BASE}/me/player/play"));
            request = request
                .header("Authorization", format!("Bearer {}", token.value))
                .header("Content-Type", "application/json");
            for (key, value) in &query {
                request = request.query(*key, value.as_str());
            }
            let response = request.send(body).map_err(AppError::from)?;
            return finish_empty(response);
        }
        self.put_empty("/me/player/play", &query)
    }

    /// `PUT /me/player/pause`
    pub fn pause(&self, device_id: Option<&str>) -> AppResult<()> {
        let mut query: Vec<(&str, String)> = Vec::new();
        if let Some(device) = device_id {
            query.push(("device_id", device.to_string()));
        }
        self.put_empty("/me/player/pause", &query)
    }

    /// `POST /me/player/next`
    pub fn next(&self, device_id: Option<&str>) -> AppResult<()> {
        let mut query: Vec<(&str, String)> = Vec::new();
        if let Some(device) = device_id {
            query.push(("device_id", device.to_string()));
        }
        self.post_empty("/me/player/next", &query)
    }

    /// `POST /me/player/previous`
    pub fn previous(&self, device_id: Option<&str>) -> AppResult<()> {
        let mut query: Vec<(&str, String)> = Vec::new();
        if let Some(device) = device_id {
            query.push(("device_id", device.to_string()));
        }
        self.post_empty("/me/player/previous", &query)
    }

    /// `PUT /me/player/seek`
    pub fn seek(&self, position_ms: u64, device_id: Option<&str>) -> AppResult<()> {
        let mut query: Vec<(&str, String)> = vec![("position_ms", position_ms.to_string())];
        if let Some(device) = device_id {
            query.push(("device_id", device.to_string()));
        }
        self.put_empty("/me/player/seek", &query)
    }

    /// `PUT /me/player/volume`
    pub fn volume(&self, percent: u8, device_id: Option<&str>) -> AppResult<()> {
        let mut query: Vec<(&str, String)> = vec![("volume_percent", percent.to_string())];
        if let Some(device) = device_id {
            query.push(("device_id", device.to_string()));
        }
        self.put_empty("/me/player/volume", &query)
    }

    /// `PUT /me/player/shuffle`
    pub fn shuffle(&self, state: bool, device_id: Option<&str>) -> AppResult<()> {
        let mut query: Vec<(&str, String)> = vec![("state", state.to_string())];
        if let Some(device) = device_id {
            query.push(("device_id", device.to_string()));
        }
        self.put_empty("/me/player/shuffle", &query)
    }

    /// `PUT /me/player/repeat`
    pub fn repeat(&self, state: &str, device_id: Option<&str>) -> AppResult<()> {
        let mut query: Vec<(&str, String)> = vec![("state", state.to_string())];
        if let Some(device) = device_id {
            query.push(("device_id", device.to_string()));
        }
        self.put_empty("/me/player/repeat", &query)
    }

    /// `POST /me/player` — move playback to another device.
    pub fn transfer(&self, device_id: &str, play: bool) -> AppResult<()> {
        let token = self.access_token()?;
        let body = serde_json::json!({
            "device_ids": [device_id],
            "play": play,
        })
        .to_string();

        let mut request = self.agent.put(format!("{API_BASE}/me/player"));
        request = request
            .header("Authorization", format!("Bearer {}", token.value))
            .header("Content-Type", "application/json");
        let response = request.send(body).map_err(AppError::from)?;
        finish_empty(response)
    }

    fn post_empty(&self, path: &str, query: &[(&str, String)]) -> AppResult<()> {
        let token = self.access_token()?;
        let url = format!("{API_BASE}{path}");

        for attempt in 0..2 {
            let used_token = if attempt == 0 {
                token.value.clone()
            } else {
                self.refresh_access_token()?.value
            };
            let mut request = self.agent.post(&url);
            request = request.header("Authorization", format!("Bearer {used_token}"));
            for (key, value) in query {
                request = request.query(*key, value.as_str());
            }

            let response = match request.send_empty() {
                Ok(response) => response,
                Err(ureq::Error::StatusCode(401)) => continue,
                Err(err) => return Err(AppError::from(err)),
            };

            let status = status_of(&response);
            if status == 401 && attempt == 0 {
                continue;
            }
            if status == 429 {
                let seconds = retry_after(&response).unwrap_or(30);
                return Err(AppError::RateLimited(seconds));
            }
            if status >= 400 {
                let body = response.into_body().read_to_string().unwrap_or_default();
                return Err(map_status(status, &body));
            }
            return Ok(());
        }

        Err(AppError::Unauthorized)
    }
}

fn finish_empty(response: ureq::http::Response<ureq::Body>) -> AppResult<()> {
    let status = status_of(&response);
    if status == 429 {
        let seconds = retry_after(&response).unwrap_or(30);
        return Err(AppError::RateLimited(seconds));
    }
    if status >= 400 {
        let body = response.into_body().read_to_string().unwrap_or_default();
        return Err(map_status(status, &body));
    }
    Ok(())
}

/// Maps an HTTP status plus body into the most useful error variant.
fn map_status(status: u16, body: &str) -> AppError {
    let detail = spotify_error_message(body).unwrap_or_default();
    match status {
        401 => AppError::Unauthorized,
        403 => {
            // Spotify uses 403 with "restriction" for Premium-only actions and
            // for device restrictions; the message is the only discriminator.
            AppError::Forbidden(detail)
        }
        404 => AppError::NoActiveDevice,
        429 => AppError::RateLimited(30),
        502 | 503 | 504 => {
            AppError::Network(format!("Spotify is temporarily unavailable ({status})"))
        }
        _ => AppError::Network(format!(
            "Spotify returned HTTP {status}{}",
            if detail.is_empty() {
                String::new()
            } else {
                format!(": {detail}")
            }
        )),
    }
}

fn string_field(value: &serde_json::Value, key: &str) -> String {
    value
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

/// Account profile fields Sonic displays.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub display_name: String,
    pub avatar_url: String,
    pub product: String,
}

/// A playback device, as returned by Spotify.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub is_active: bool,
    pub is_restricted: bool,
    pub volume_percent: u8,
    pub supports_volume: bool,
}

fn parse_device(value: &serde_json::Value) -> Device {
    Device {
        id: string_field(value, "id"),
        name: string_field(value, "name"),
        kind: string_field(value, "type"),
        is_active: value
            .get("is_active")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        is_restricted: value
            .get("is_restricted")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        volume_percent: value
            .get("volume_percent")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u8,
        supports_volume: value
            .get("supports_volume")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
    }
}

/// The currently playing item.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: String,
    pub uri: String,
    pub name: String,
    /// Join of all artist names.
    pub artists: Vec<String>,
    pub album: String,
    pub album_id: String,
    pub artwork_url: String,
    /// Thumbnail suitable for the mini mode, if Spotify provided one.
    pub artwork_small_url: String,
    pub duration_ms: u64,
    pub explicit: bool,
    /// `track` or `episode`.
    pub kind: String,
    /// Free-form URL to open this item in Spotify.
    pub external_url: String,
}

fn parse_item(value: &serde_json::Value) -> Item {
    let album = value
        .get("album")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let images = album
        .get("images")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    // Spotify returns images largest-first; take the first and last available.
    let artwork_url = images
        .iter()
        .find_map(|i| i.get("url").and_then(|u| u.as_str()))
        .unwrap_or("")
        .to_string();
    let artwork_small_url = images
        .iter()
        .rev()
        .find_map(|i| i.get("url").and_then(|u| u.as_str()))
        .unwrap_or(&artwork_url)
        .to_string();

    let artists = value
        .get("artists")
        .and_then(|v| v.as_array())
        .map(|list| {
            list.iter()
                .filter_map(|a| a.get("name").and_then(|n| n.as_str()))
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let kind = if value.get("show").is_some() {
        "episode".to_string()
    } else {
        string_field(value, "type")
    };

    Item {
        id: string_field(value, "id"),
        uri: string_field(value, "uri"),
        name: string_field(value, "name"),
        artists,
        album: string_field(&album, "name"),
        album_id: string_field(&album, "id"),
        artwork_url,
        artwork_small_url,
        duration_ms: value
            .get("duration_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        explicit: value
            .get("explicit")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        kind,
        external_url: value
            .pointer("/external_urls/spotify")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
    }
}

/// A full playback snapshot for the UI.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Playback {
    pub is_playing: bool,
    pub progress_ms: u64,
    pub device: Device,
    pub item: Option<Item>,
    pub shuffle_state: bool,
    pub repeat_state: String,
}
