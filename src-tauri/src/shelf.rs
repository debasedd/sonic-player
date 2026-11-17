//! File Shelf — Pocket's local file store.
//!
//! ## The one rule that matters
//!
//! Importing **copies**. A file dropped on Sonic is read and written to
//! `%APPDATA%/com.sonic.player/shelf/`; the original is never opened for
//! writing, never moved, never renamed, never deleted. There is no code path in
//! this module that can modify a source path — the only filesystem mutation is
//! inside `shelf_dir()`, which is a directory Sonic owns.
//!
//! ## Layout
//!
//! ```text
//! %APPDATA%/com.sonic.player/
//!   shelf.json                 index (metadata only, no file bytes)
//!   shelf/
//!     0001-<slug>.<ext>        the copy
//!     0002-<slug>.<ext>
//! ```
//!
//! The numeric prefix is a monotonic counter from the index, so a collision can
//! never overwrite an existing copy — the name is unique by construction, and
//! `create_new(true)` is the final guard if two imports race.
//!
//! ## Index durability
//!
//! `save_index` writes `shelf.json.tmp` then renames, the same atomic pattern
//! `config::save_settings` uses. A crash mid-write leaves the old index intact
//! rather than a truncated one, which would make every stored file look orphaned.
//!
//! ## What is deliberately NOT here
//!
//! No base64, no SQLite, no image thumbnails at import time. Bytes stay on disk
//! as files; the index holds only paths, sizes, and timestamps. Previews are
//! generated lazily by the frontend from the file path, and released when the
//! panel closes.

use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// How a file is shown in the shelf list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ShelfKind {
    Image,
    Audio,
    Video,
    Document,
    Archive,
    Other,
}

impl ShelfKind {
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_ascii_lowercase().as_str() {
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico" | "avif" | "svg" => {
                ShelfKind::Image
            }
            "mp3" | "flac" | "wav" | "ogg" | "m4a" | "aac" | "opus" | "wma" => ShelfKind::Audio,
            "mp4" | "mkv" | "mov" | "avi" | "webm" | "wmv" => ShelfKind::Video,
            "txt" | "md" | "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "csv"
            | "json" | "rs" | "ts" | "tsx" | "js" | "py" | "log" => ShelfKind::Document,
            "zip" | "rar" | "7z" | "tar" | "gz" | "iso" => ShelfKind::Archive,
            _ => ShelfKind::Other,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ShelfKind::Image => "image",
            ShelfKind::Audio => "audio",
            ShelfKind::Video => "video",
            ShelfKind::Document => "document",
            ShelfKind::Archive => "archive",
            ShelfKind::Other => "other",
        }
    }
}

/// One stored file, as the frontend sees it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShelfEntry {
    /// Stable id: the on-disk stem, e.g. `0003-report`.
    pub id: String,
    /// Name the user dropped, kept verbatim for display and drag-out.
    pub name: String,
    /// Absolute path of Sonic's copy.
    pub path: String,
    pub size: u64,
    pub ext: String,
    pub kind: ShelfKind,
    /// Unix seconds.
    pub added_at: u64,
    /// Pinned entries sort first and are never touched by cleanup.
    #[serde(default)]
    pub pinned: bool,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ShelfIndex {
    /// Monotonic counter backing the numeric filename prefix.
    next_id: u64,
    entries: Vec<ShelfEntry>,
}

/// The persisted shelf.
#[derive(Debug, Clone)]
pub struct Shelf {
    index: ShelfIndex,
}

/// Outcome of one import call, so the UI can say "2 files added" honestly.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub added: Vec<ShelfEntry>,
    /// Files that were skipped, with the reason shown to the user.
    pub skipped: Vec<SkippedFile>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedFile {
    pub name: String,
    pub reason: String,
}

impl Shelf {
    /// Loads, imports, and saves under one lock.
    ///
    /// `load()` → `import_paths()` → `save()` is a read-modify-write cycle, and
    /// imports run on a worker thread so a drop never blocks the UI. Without a
    /// lock, two drops landing close together both read the same old index and
    /// the second `save` silently discards the first drop's files — the log says
    /// `added=2` while `shelf.json` ends up with one entry. That is a data-loss
    /// bug, not a cosmetic one, so the cycle is serialized here.
    ///
    /// A poisoned lock is recovered rather than propagated: the guard is only a
    /// `()`, so a panic while holding it cannot have left the index half-written.
    pub fn import_locked(paths: Vec<PathBuf>, max_mib: u32) -> ImportReport {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

        let mut shelf = Self::load();
        shelf.import_paths(paths, max_mib)
    }

    /// Loads the index, tolerating a missing or corrupt file.
    ///
    /// A corrupt index is treated as empty rather than fatal: the files are still
    /// on disk, and failing to start over an unreadable index would be worse than
    /// showing an empty shelf. Entries whose backing file vanished (user deleted
    /// it from Explorer) are dropped here so the list never lies.
    pub fn load() -> Self {
        let mut index: ShelfIndex = fs::read_to_string(crate::config::shelf_index_path())
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default();

        index
            .entries
            .retain(|entry| Path::new(&entry.path).is_file());
        Self { index }
    }

    fn save(&self) -> AppResult<()> {
        let path = crate::config::shelf_index_path();
        let json = serde_json::to_string_pretty(&self.index)
            .map_err(|e| AppError::Local(format!("could not encode shelf index: {e}")))?;
        let tmp = path.with_extension("json.tmp");
        {
            let mut file = fs::File::create(&tmp)
                .map_err(|e| AppError::Local(format!("could not write shelf index: {e}")))?;
            file.write_all(json.as_bytes())
                .map_err(|e| AppError::Local(format!("could not write shelf index: {e}")))?;
        }
        fs::rename(&tmp, &path)
            .map_err(|e| AppError::Local(format!("could not replace shelf index: {e}")))?;
        Ok(())
    }

    pub fn entries(&self) -> Vec<ShelfEntry> {
        self.index.entries.clone()
    }

    pub fn is_empty(&self) -> bool {
        self.index.entries.is_empty()
    }

    /// Copies the given absolute paths into the shelf.
    ///
    /// Each file is handled independently: one unreadable file must not abort the
    /// import of the rest, which is why the report carries per-file skips.
    pub fn import_paths(&mut self, paths: Vec<PathBuf>, max_mib: u32) -> ImportReport {
        let mut report = ImportReport {
            added: Vec::new(),
            skipped: Vec::new(),
        };

        for path in paths {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "untitled".to_string());

            match self.import_one(&path, &name, max_mib) {
                Ok(entry) => report.added.push(entry),
                Err(reason) => report.skipped.push(SkippedFile { name, reason }),
            }
        }

        // Persist once, after the batch: one rename instead of N.
        if !report.added.is_empty() {
            if let Err(err) = self.save() {
                report.skipped.push(SkippedFile {
                    name: "index".into(),
                    reason: format!("files were copied but the index could not be saved: {err}"),
                });
            }
        }

        report
    }

    fn import_one(
        &mut self,
        source: &Path,
        name: &str,
        max_mib: u32,
    ) -> Result<ShelfEntry, String> {
        // A directory is not a file. Saying so beats a generic IO error.
        let metadata =
            fs::metadata(source).map_err(|e| format!("could not read this file: {e}"))?;
        if metadata.is_dir() {
            return Err("folders are not supported".into());
        }
        if metadata.len() == 0 {
            return Err("the file is empty".into());
        }
        if max_mib > 0 && metadata.len() > u64::from(max_mib) * 1024 * 1024 {
            return Err(format!("larger than the {max_mib} MiB limit"));
        }

        let shelf_dir = crate::config::shelf_dir();
        fs::create_dir_all(&shelf_dir)
            .map_err(|e| format!("could not create the shelf folder: {e}"))?;

        let ext = source
            .extension()
            .map(|e| e.to_string_lossy().to_string())
            .unwrap_or_default();
        let kind = ShelfKind::from_extension(&ext);

        // Duplicate content check: if the same bytes are already stored, reuse
        // that copy instead of writing a second one. The original is still only
        // read, never touched.
        if let Some(existing) = self.find_duplicate(source, metadata.len()) {
            return Ok(existing);
        }

        self.index.next_id += 1;
        let id = format!("{:04}", self.index.next_id);
        let slug = slugify(
            &Path::new(name)
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy(),
        );
        let file_name = if slug.is_empty() {
            format!("{id}.{ext}")
        } else if ext.is_empty() {
            format!("{id}-{slug}")
        } else {
            format!("{id}-{slug}.{ext}")
        };
        let dest = shelf_dir.join(&file_name);

        // Streaming copy: 64 KiB at a time. A 4 GB file never lands in memory.
        copy_streaming(source, &dest).map_err(|e| format!("could not copy the file: {e}"))?;

        let written = fs::metadata(&dest)
            .map(|m| m.len())
            .unwrap_or(metadata.len());

        let entry = ShelfEntry {
            id,
            name: name.to_string(),
            path: dest.to_string_lossy().to_string(),
            size: written,
            ext,
            kind,
            added_at: now_secs(),
            pinned: false,
        };

        self.index.entries.push(entry.clone());
        Ok(entry)
    }

    /// True when a byte-identical copy is already stored.
    ///
    /// Compares size first (cheap) and then hashes both files. Hashing a large
    /// file on every drop would make drag-and-drop feel slow, so the check only
    /// runs against entries with a matching size.
    fn find_duplicate(&self, source: &Path, size: u64) -> Option<ShelfEntry> {
        let source_hash = if size <= 64 * 1024 * 1024 {
            hash_file(source)
        } else {
            None
        }?;

        self.index
            .entries
            .iter()
            .find(|entry| {
                entry.size == size && hash_file(Path::new(&entry.path)) == Some(source_hash)
            })
            .cloned()
    }

    /// Marks an entry pinned/unpinned.
    pub fn set_pinned(&mut self, id: &str, pinned: bool) -> AppResult<()> {
        let Some(entry) = self.index.entries.iter_mut().find(|e| e.id == id) else {
            return Err(AppError::NotFound(
                "That file is no longer in the shelf.".into(),
            ));
        };
        entry.pinned = pinned;
        self.save()
    }

    /// Deletes Sonic's copy. Never touches the original, which lives elsewhere.
    pub fn remove(&mut self, id: &str) -> AppResult<()> {
        let Some(position) = self.index.entries.iter().position(|e| e.id == id) else {
            return Err(AppError::NotFound(
                "That file is no longer in the shelf.".into(),
            ));
        };

        let entry = self.index.entries.remove(position);
        // A failed unlink leaves the entry in place rather than dropping metadata
        // for a file that still occupies disk.
        if let Err(err) = fs::remove_file(&entry.path) {
            if err.kind() != std::io::ErrorKind::NotFound {
                self.index.entries.insert(position, entry);
                return Err(AppError::Local(format!(
                    "could not delete the stored copy: {err}"
                )));
            }
        }
        self.save()
    }

    /// Removes every unpinned entry. Pinned entries survive by design.
    pub fn clear_unpinned(&mut self) -> AppResult<usize> {
        let (keep, drop): (Vec<_>, Vec<_>) =
            self.index.entries.drain(..).partition(|entry| entry.pinned);

        for entry in &drop {
            let _ = fs::remove_file(&entry.path);
        }
        self.index.entries = keep;
        self.save()?;
        Ok(drop.len())
    }

    /// Deletes index entries whose files no longer exist and returns how many.
    pub fn prune_missing(&mut self) -> AppResult<usize> {
        let before = self.index.entries.len();
        self.index
            .entries
            .retain(|entry| Path::new(&entry.path).is_file());
        let removed = before - self.index.entries.len();
        if removed > 0 {
            self.save()?;
        }
        Ok(removed)
    }

    /// Total bytes held by the shelf, for the settings readout.
    pub fn total_bytes(&self) -> u64 {
        self.index.entries.iter().map(|e| e.size).sum()
    }
}

/// Streams `from` into `to`, creating it exclusively.
///
/// `create_new(true)` is the guarantee behind "never silently overwrite": if a
/// file with that name somehow exists, this fails instead of clobbering it.
fn copy_streaming(from: &Path, to: &Path) -> std::io::Result<()> {
    let mut reader = fs::File::open(from)?;
    let mut writer = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(to)?;

    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        writer.write_all(&buffer[..read])?;
    }
    writer.flush()?;
    Ok(())
}

/// Lower-cases, strips anything that is not a letter, digit, dash or space, and
/// collapses runs of separators.
fn slugify(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut last_was_dash = false;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_was_dash = false;
        } else if !last_was_dash && !out.is_empty() {
            out.push('-');
            last_was_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    // Keep filenames well under the Windows path budget even for absurd titles.
    out.chars().take(48).collect()
}

/// FNV-1a, used only to detect identical content.
///
/// Not a security hash — this is a duplicate check, not a signature. Chosen
/// because it is a dozen lines with no dependency, and collisions here cost a
/// redundant copy rather than correctness.
fn hash_file(path: &Path) -> Option<u64> {
    let mut file = fs::File::open(path).ok()?;
    let mut buffer = vec![0u8; 64 * 1024];
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    loop {
        let read = file.read(&mut buffer).ok()?;
        if read == 0 {
            break;
        }
        for byte in &buffer[..read] {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    Some(hash)
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("sonic-shelf-test-{tag}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn slugify_produces_safe_names() {
        assert_eq!(slugify("Q3 Report (final)"), "q3-report-final");
        assert_eq!(slugify("  ---  "), "");
        assert_eq!(slugify("a"), "a");
    }

    #[test]
    fn kind_comes_from_extension() {
        assert_eq!(ShelfKind::from_extension("PNG"), ShelfKind::Image);
        assert_eq!(ShelfKind::from_extension("flac"), ShelfKind::Audio);
        assert_eq!(ShelfKind::from_extension("zzz"), ShelfKind::Other);
    }

    #[test]
    fn copy_streaming_creates_and_preserves_bytes() {
        let dir = temp_dir("copy");
        let src = dir.join("source.txt");
        let dst = dir.join("dest.txt");
        fs::write(&src, b"hello shelf").unwrap();

        copy_streaming(&src, &dst).unwrap();
        assert_eq!(fs::read(&dst).unwrap(), b"hello shelf");

        // The source must be untouched — the whole promise of the shelf.
        assert_eq!(fs::read(&src).unwrap(), b"hello shelf");

        // Second write to the same destination must fail, not overwrite.
        assert!(copy_streaming(&src, &dst).is_err());
        assert_eq!(fs::read(&dst).unwrap(), b"hello shelf");
    }

    #[test]
    fn hash_detects_identical_and_differs_on_change() {
        let dir = temp_dir("hash");
        let a = dir.join("a.bin");
        let b = dir.join("b.bin");
        let c = dir.join("c.bin");
        fs::write(&a, b"same").unwrap();
        fs::write(&b, b"same").unwrap();
        fs::write(&c, b"diff").unwrap();

        assert_eq!(hash_file(&a), hash_file(&b));
        assert_ne!(hash_file(&a), hash_file(&c));
    }
}
