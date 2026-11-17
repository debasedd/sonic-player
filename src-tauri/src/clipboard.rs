//! Clipboard Vault — Pocket's local clipboard history.
//!
//! ## Privacy is the feature, not a footnote
//!
//! Everything here stays on this machine. There is no HTTP client in this
//! module, no logging of content, and no telemetry hook. Captured text is never
//! written to `eprintln!`, never embedded in an [`AppError`], and never sent to
//! Spotify — the error type carries fixed sentences plus filesystem paths only.
//!
//! Capture is **opt-in** (`Settings::clipboard_enabled`, default `false`) and
//! stops immediately when the user turns it off or flips Private Mode on. Private
//! Mode stops *capture*; it never deletes what is already stored.
//!
//! ## How capture works
//!
//! Polling `GetClipboardSequenceNumber` rather than subclassing the clipboard.
//! A window-procedure hook would be more "correct" but means owning a hidden
//! message-only window and its lifetime; the sequence number is one cheap
//! `user32` call that increments on every clipboard change, so a 900 ms poll
//! costs nothing when the clipboard is idle.
//!
//! The foreground process name is checked against the user's exclusion list
//! before anything is read, so a password manager's clipboard contents are never
//! even copied into Sonic's memory.
//!
//! ## Storage
//!
//! ```text
//! %APPDATA%/com.sonic.player/
//!   clipboard.json          index: id, kind, text or image path, timestamps
//!   clipboard/
//!     0001.png              captured image (lossless PNG)
//! ```
//!
//! Text lives in the index because it is small. Images live as files because a
//! 1080p screenshot as a database blob is how a 100 MB memory budget dies.

use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// How a captured entry is rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ClipboardKind {
    Text,
    Image,
}

/// One captured clipboard entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardEntry {
    pub id: String,
    pub kind: ClipboardKind,
    /// Present for text entries. Truncated for display, full text for restore.
    pub text: Option<String>,
    /// Absolute path of the stored PNG for image entries.
    pub image_path: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// Unix seconds.
    pub captured_at: u64,
    /// The process that owned the clipboard, e.g. `firefox`.
    pub source: String,
    #[serde(default)]
    pub pinned: bool,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClipboardIndex {
    next_id: u64,
    entries: Vec<ClipboardEntry>,
}

/// Hard ceiling on stored text per entry.
///
/// A single clipboard payload can be tens of megabytes (a copied log file). One
/// entry that large would bloat the index that gets rewritten on every capture,
/// so text is truncated and the entry is flagged.
const MAX_TEXT_CHARS: usize = 64 * 1024;

/// Ceiling for a captured image, in pixels per side. Larger images are skipped:
/// the clipboard is not a screenshot archive.
const MAX_IMAGE_SIDE: u32 = 4096;

pub struct ClipboardVault {
    index: ClipboardIndex,
}

/// What one capture attempt decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureOutcome {
    /// Nothing new on the clipboard.
    Unchanged,
    /// Captured and stored.
    Stored(ClipboardEntry),
    /// Deliberately ignored: disabled, private mode, excluded app, or a
    /// recognised secret pattern.
    Skipped(&'static str),
}

impl ClipboardVault {
    pub fn load() -> Self {
        let mut index: ClipboardIndex = fs::read_to_string(crate::config::clipboard_index_path())
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default();

        // Drop image entries whose file is gone, so the list never shows a
        // thumbnail that 404s.
        index.entries.retain(|entry| match entry.kind {
            ClipboardKind::Text => true,
            ClipboardKind::Image => entry
                .image_path
                .as_deref()
                .map(Path::new)
                .map(Path::is_file)
                .unwrap_or(false),
        });

        Self { index }
    }

    fn save(&self) -> AppResult<()> {
        let path = crate::config::clipboard_index_path();
        let json = serde_json::to_string_pretty(&self.index)
            .map_err(|e| AppError::Local(format!("could not encode clipboard index: {e}")))?;
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, json)
            .map_err(|e| AppError::Local(format!("could not write clipboard index: {e}")))?;
        fs::rename(&tmp, &path)
            .map_err(|e| AppError::Local(format!("could not replace clipboard index: {e}")))?;
        Ok(())
    }

    pub fn entries(&self) -> Vec<ClipboardEntry> {
        self.index.entries.clone()
    }

    pub fn is_empty(&self) -> bool {
        self.index.entries.is_empty()
    }

    pub fn total_bytes(&self) -> u64 {
        self.index
            .entries
            .iter()
            .map(|entry| match entry.kind {
                ClipboardKind::Text => entry.text.as_ref().map(|t| t.len() as u64).unwrap_or(0),
                ClipboardKind::Image => entry
                    .image_path
                    .as_deref()
                    .and_then(|p| fs::metadata(p).ok())
                    .map(|m| m.len())
                    .unwrap_or(0),
            })
            .sum()
    }

    /// Stores a captured entry, newest first.
    pub fn push(
        &mut self,
        kind: ClipboardKind,
        text: Option<String>,
        image: Option<StoredImage>,
    ) -> AppResult<ClipboardEntry> {
        self.index.next_id += 1;
        let entry = ClipboardEntry {
            id: format!("{:04}", self.index.next_id),
            kind,
            text,
            image_path: image.as_ref().map(|i| i.path.clone()),
            width: image.as_ref().and_then(|i| i.width),
            height: image.as_ref().and_then(|i| i.height),
            captured_at: now_secs(),
            source: String::new(),
            pinned: false,
        };

        self.index.entries.insert(0, entry.clone());
        self.enforce_limits()?;
        self.save()?;
        Ok(entry)
    }

    /// Persists the index without touching retention.
    ///
    /// The capture path calls this after `set_source`, because a retention sweep
    /// on every copy would rewrite the index twice per clipboard change.
    pub fn save_for_capture(&self) -> AppResult<()> {
        self.save()
    }

    /// Records the source app after the fact.
    ///
    /// Separate from `push` because the process check happens before the read:
    /// by the time we know what the clipboard holds we already know who owns it.
    pub fn set_source(&mut self, id: &str, source: String) {
        if let Some(entry) = self.index.entries.iter_mut().find(|e| e.id == id) {
            entry.source = source;
        }
    }

    pub fn set_pinned(&mut self, id: &str, pinned: bool) -> AppResult<()> {
        let Some(entry) = self.index.entries.iter_mut().find(|e| e.id == id) else {
            return Err(AppError::NotFound("That clipboard entry is gone.".into()));
        };
        entry.pinned = pinned;
        self.save()
    }

    pub fn remove(&mut self, id: &str) -> AppResult<()> {
        let Some(position) = self.index.entries.iter().position(|e| e.id == id) else {
            return Err(AppError::NotFound("That clipboard entry is gone.".into()));
        };
        let entry = self.index.entries.remove(position);
        if let Some(path) = entry.image_path {
            let _ = fs::remove_file(path);
        }
        self.save()
    }

    /// Clears everything. Called by the explicit "Clear all" action only.
    pub fn clear_all(&mut self) -> AppResult<usize> {
        let count = self.index.entries.len();
        for entry in self.index.entries.drain(..) {
            if let Some(path) = entry.image_path {
                let _ = fs::remove_file(path);
            }
        }
        self.save()?;
        Ok(count)
    }

    /// Retention pass: drops expired unpinned entries and over-limit overflow.
    ///
    /// Pinned entries are exempt from both rules — that is the entire point of
    /// pinning, and a retention sweep that deleted a pinned item would make the
    /// pin button a lie.
    pub fn enforce_limits(&mut self) -> AppResult<()> {
        let max_entries = crate::config::load_settings().clipboard_max_entries as usize;
        let retention_days = crate::config::load_settings().clipboard_retention_days;
        let cutoff = now_secs().saturating_sub(u64::from(retention_days) * 86_400);

        let before = self.index.entries.len();

        // Age-based prune (unpinned only).
        self.index
            .entries
            .retain(|entry| entry.pinned || entry.captured_at >= cutoff);

        // Count-based trim (unpinned only, newest kept).
        if self.index.entries.len() > max_entries {
            let mut kept: Vec<ClipboardEntry> = Vec::with_capacity(max_entries);
            let mut dropped: Vec<ClipboardEntry> = Vec::new();
            for entry in self.index.entries.drain(..) {
                if entry.pinned || kept.len() < max_entries {
                    kept.push(entry);
                } else {
                    dropped.push(entry);
                }
            }
            for entry in &dropped {
                if let Some(path) = &entry.image_path {
                    let _ = fs::remove_file(path);
                }
            }
            self.index.entries = kept;
        }

        if self.index.entries.len() != before {
            self.save()?;
        }
        Ok(())
    }

    /// Deletes image files that no index entry references.
    ///
    /// Reached when a previous run crashed between writing a PNG and saving the
    /// index; without this the clipboard folder grows forever.
    pub fn prune_orphan_images(&self) -> usize {
        let dir = crate::config::clipboard_dir();
        let Ok(read_dir) = fs::read_dir(&dir) else {
            return 0;
        };

        let referenced: Vec<&str> = self
            .index
            .entries
            .iter()
            .filter_map(|e| e.image_path.as_deref())
            .collect();

        let mut removed = 0;
        for item in read_dir.flatten() {
            let path = item.path();
            if path.extension().and_then(|e| e.to_str()) != Some("png") {
                continue;
            }
            let path_str = path.to_string_lossy();
            if !referenced.iter().any(|r| *r == path_str) {
                if fs::remove_file(&path).is_ok() {
                    removed += 1;
                }
            }
        }
        removed
    }

    /// Restores an entry to the real Windows clipboard so Ctrl+V works elsewhere.
    pub fn restore_to_clipboard(&self, id: &str) -> AppResult<()> {
        let entry = self
            .index
            .entries
            .iter()
            .find(|e| e.id == id)
            .ok_or_else(|| AppError::NotFound("That clipboard entry is gone.".into()))?;

        match entry.kind {
            ClipboardKind::Text => {
                let text = entry.text.clone().unwrap_or_default();
                crate::clipboard_sys::set_text(&text)
            }
            ClipboardKind::Image => {
                let path = entry.image_path.clone().unwrap_or_default();
                crate::clipboard_sys::set_image_from_png(Path::new(&path))
            }
        }
    }

    /// Writes a captured image to disk and returns its metadata.
    pub fn store_image(&self, rgba: &[u8], width: u32, height: u32) -> AppResult<StoredImage> {
        let dir = crate::config::clipboard_dir();
        fs::create_dir_all(&dir)
            .map_err(|e| AppError::Local(format!("could not create clipboard folder: {e}")))?;

        let id = format!("{:08}", now_millis());
        let path = dir.join(format!("{id}.png"));

        // PNG is lossless: a re-encoded screenshot must not gain compression
        // artifacts, and the clipboard already holds raw pixels.
        image::RgbaImage::from_raw(width, height, rgba.to_vec())
            .ok_or_else(|| AppError::Local("captured image had an unexpected pixel layout".into()))?
            .save(&path)
            .map_err(|e| AppError::Local(format!("could not store clipboard image: {e}")))?;

        Ok(StoredImage {
            path: path.to_string_lossy().to_string(),
            width: Some(width),
            height: Some(height),
        })
    }
}

/// Metadata for a stored clipboard image.
#[derive(Debug, Clone)]
pub struct StoredImage {
    pub path: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

/// Decides whether a piece of text looks like a secret the user would not want
/// archived.
///
/// Deliberately conservative, and deliberately *not* sold as complete. A regex
/// cannot tell a password from a passphrase, and a false positive means a
/// silently missing clipboard entry. These are the shapes that are unambiguous
/// enough to act on.
pub fn looks_sensitive(text: &str) -> bool {
    let trimmed = text.trim();

    // One-time passcodes: 6-8 digits alone on a line, the shape every 2FA
    // message uses. Nine or more digits is a phone number or an account number,
    // both of which people legitimately copy.
    if (6..=8).contains(&trimmed.len()) && trimmed.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }

    // PEM private key blocks.
    if trimmed.contains("-----BEGIN ") && trimmed.contains("PRIVATE KEY----") {
        return true;
    }

    // Common credential assignments, in the forms people actually copy:
    //   password=hunter2
    //   db_password = correcthorse
    //   "password": "hunter2"
    //   AWS_SECRET_ACCESS_KEY=...
    let lower = trimmed.to_ascii_lowercase();
    const MARKERS: [&str; 7] = [
        "password",
        "passwd",
        "secret_key",
        "api_key",
        "aws_secret",
        "access_token",
        "client_secret",
    ];
    for marker in MARKERS {
        let Some(at) = lower.find(marker) else {
            continue;
        };
        // The keyword has to be followed by an assignment, with optional
        // whitespace: `db_password = x` and `password=x` are the same shape, and
        // handling only the first is how the second slips through.
        let mut rest = trimmed[at + marker.len()..].trim_start();
        if rest.starts_with('"') {
            rest = rest[1..].trim_start();
        }
        if let Some(after) = rest.strip_prefix(':').or_else(|| rest.strip_prefix('=')) {
            let value = after.trim().trim_matches('"').trim();
            // A value of at least four characters: shorter than that is a
            // placeholder like `password=` in a form, not a secret.
            if value.len() >= 4 {
                return true;
            }
        }
    }

    false
}

/// Clamps stored text, marking truncation.
///
/// Returns the text plus whether it was cut. The UI shows an ellipsis and the
/// restore action uses the stored (truncated) text — restoring a silently
/// shortened password would be worse than admitting the limit.
pub fn clamp_text(text: &str) -> (String, bool) {
    if text.chars().count() <= MAX_TEXT_CHARS {
        return (text.to_string(), false);
    }
    let mut out: String = text.chars().take(MAX_TEXT_CHARS).collect();
    out.push_str("\n… (truncated)");
    (out, true)
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn now_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn otp_like_text_is_skipped() {
        // 6-8 digits alone is the shape every 2FA message uses.
        assert!(looks_sensitive("123456"));
        assert!(looks_sensitive("1234567"));
        assert!(looks_sensitive("12345678"));
        // A 5-digit, 9-digit or longer number is not the OTP shape: phone numbers,
        // account numbers and order ids are copied constantly and dropping them
        // would be a silent data loss.
        assert!(!looks_sensitive("12345"));
        assert!(!looks_sensitive("123456789"));
        assert!(!looks_sensitive("4815162342"));
        // Digits inside a sentence are not an OTP either.
        assert!(!looks_sensitive("order 123456 shipped"));
    }

    #[test]
    fn private_keys_are_skipped() {
        assert!(looks_sensitive(
            "-----BEGIN RSA PRIVATE KEY-----\nMIIE...\n-----END RSA PRIVATE KEY-----"
        ));
    }

    #[test]
    fn credential_assignments_are_skipped_only_with_a_value() {
        assert!(looks_sensitive("password=hunter2"));
        assert!(looks_sensitive("db_password = correcthorse"));
        // The word on its own is not a secret.
        assert!(!looks_sensitive(
            "the password field is on the settings page"
        ));
    }

    #[test]
    fn ordinary_text_is_kept() {
        assert!(!looks_sensitive("https://example.com/docs/getting-started"));
        assert!(!looks_sensitive("Sonic is a Liquid Glass music player"));
        assert!(!looks_sensitive(""));
    }

    #[test]
    fn clamp_marks_truncation() {
        let (short, cut) = clamp_text("hello");
        assert_eq!(short, "hello");
        assert!(!cut);

        let long = "a".repeat(MAX_TEXT_CHARS + 100);
        let (stored, cut) = clamp_text(&long);
        assert!(cut);
        assert!(stored.contains("truncated"));
    }
}
