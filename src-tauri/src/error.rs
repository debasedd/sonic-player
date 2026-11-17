//! Error types shared by the Spotify client and the command layer.
//!
//! Commands return a plain string so the frontend can render a human sentence
//! without knowing anything about Rust enums.

use serde::Serialize;

/// A failure that can be surfaced to the UI.
#[derive(Debug, Clone)]
pub enum AppError {
    /// Spotify answered 401 even after a refresh attempt: re-auth required.
    Unauthorized,
    /// Spotify answered 403 (Premium required, or a restricted endpoint).
    Forbidden(String),
    /// No active device, or the requested device is gone.
    NoActiveDevice,
    /// Spotify answered 429; carries the `Retry-After` hint in seconds.
    RateLimited(u64),
    /// Device or track disappeared.
    NotFound(String),
    /// Transport-level failure (DNS, TLS, timeout, connection reset).
    Network(String),
    /// The Spotify access token could not be refreshed.
    RefreshFailed(String),
    /// A local, non-Spotify failure (registry, credential vault, IO).
    Local(String),
}

impl AppError {
    /// Stable machine-readable code for the frontend.
    pub fn code(&self) -> &'static str {
        match self {
            AppError::Unauthorized => "unauthorized",
            AppError::Forbidden(_) => "forbidden",
            AppError::NoActiveDevice => "no_active_device",
            AppError::RateLimited(_) => "rate_limited",
            AppError::NotFound(_) => "not_found",
            AppError::Network(_) => "network",
            AppError::RefreshFailed(_) => "refresh_failed",
            AppError::Local(_) => "local",
        }
    }

    /// A sentence the UI can show verbatim.
    pub fn message(&self) -> String {
        match self {
            AppError::Unauthorized => {
                "Spotify needs you to sign in again. Reconnect your account.".into()
            }
            AppError::Forbidden(detail) => {
                if detail.contains("Premium") {
                    "This control needs Spotify Premium on the active device.".into()
                } else {
                    "Spotify refused that request for this device.".into()
                }
            }
            AppError::NoActiveDevice => {
                "No Spotify device is active. Open Spotify and start playing, then try again."
                    .into()
            }
            AppError::RateLimited(seconds) => {
                format!("Spotify is rate limiting Sonic. Retrying in {seconds}s.")
            }
            AppError::NotFound(detail) => detail.clone(),
            AppError::Network(detail) => {
                format!("Could not reach Spotify. Check your connection. ({detail})")
            }
            AppError::RefreshFailed(detail) => {
                format!("Could not refresh Spotify access. {detail}")
            }
            AppError::Local(detail) => detail.clone(),
        }
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message())
    }
}

impl std::error::Error for AppError {}

/// Wire format consumed by the frontend.
impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("AppError", 3)?;
        s.serialize_field("code", self.code())?;
        s.serialize_field("message", &self.message())?;
        s.serialize_field(
            "retryAfterSeconds",
            &match self {
                AppError::RateLimited(seconds) => Some(*seconds),
                _ => None,
            },
        )?;
        s.end()
    }
}

impl From<ureq::Error> for AppError {
    fn from(err: ureq::Error) -> Self {
        match err {
            ureq::Error::StatusCode(401) => AppError::Unauthorized,
            ureq::Error::StatusCode(403) => AppError::Forbidden(String::new()),
            ureq::Error::StatusCode(404) => {
                AppError::NotFound("Spotify could not find that.".into())
            }
            ureq::Error::StatusCode(429) => AppError::RateLimited(30),
            ureq::Error::StatusCode(code) => {
                AppError::Network(format!("Spotify returned HTTP {code}"))
            }
            ureq::Error::HostNotFound => AppError::Network("host not found".into()),
            ureq::Error::Timeout(_) => AppError::Network("request timed out".into()),
            other => AppError::Network(other.to_string()),
        }
    }
}

/// Convenience alias used across the backend.
pub type AppResult<T> = Result<T, AppError>;
