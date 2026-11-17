//! Persistent configuration, paths, and Windows autostart handling.
//!
//! Nothing here is a secret except the Spotify client id, which is a public
//! value by design (PKCE has no client secret).

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

/// Environment variable consulted before the persisted config. Handy for CI and
/// for users who keep their credentials in a `.env` file while developing.
pub const CLIENT_ID_ENV: &str = "SONIC_SPOTIFY_CLIENT_ID";
/// Environment variable that pins the OAuth loopback port.
pub const REDIRECT_PORT_ENV: &str = "SONIC_SPOTIFY_REDIRECT_PORT";

/// Default loopback port. Spotify requires this exact URI to be allow-listed in
/// the developer dashboard.
pub const DEFAULT_REDIRECT_PORT: u16 = 8976;
/// Path component of the redirect URI.
pub const REDIRECT_PATH: &str = "/callback";

/// OAuth scopes Sonic actually uses.
///
/// `user-read-currently-playing` is what makes the one-second polling cheap;
/// `user-read-playback-state` covers device and shuffle/repeat metadata.
pub const SCOPES: &str = "user-read-playback-state \
user-modify-playback-state \
user-read-currently-playing \
user-read-private \
user-read-email";

/// How the player presents itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum WindowMode {
    /// The default compact pill.
    Pill,
    /// Full-height card with large artwork and extra controls.
    Expanded,
    /// Collapsed strip: artwork, play/pause, progress hairline.
    #[default]
    Mini,
}

impl WindowMode {
    pub fn as_str(self) -> &'static str {
        match self {
            WindowMode::Pill => "pill",
            WindowMode::Expanded => "expanded",
            WindowMode::Mini => "mini",
        }
    }

    pub fn from_str_lenient(value: &str) -> Self {
        match value {
            "expanded" => WindowMode::Expanded,
            "mini" => WindowMode::Mini,
            _ => WindowMode::Pill,
        }
    }

    /// Inner window size in logical pixels for this mode.
    ///
    /// Width stays constant across modes so the window never drifts sideways
    /// when the user expands or collapses it.
    pub fn size(self) -> (f64, f64) {
        match self {
            WindowMode::Pill => (416.0, 108.0),
            // Matches the measured natural height of the expanded layout.
            WindowMode::Expanded => (416.0, 548.0),
            WindowMode::Mini => (248.0, 60.0),
        }
    }
}

/// User-facing preferences. Persisted as JSON in the per-user app data dir.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// Spotify client id from the developer dashboard. Public by design.
    #[serde(default)]
    pub client_id: String,
    /// Loopback port used for the OAuth redirect.
    #[serde(default = "default_redirect_port")]
    pub redirect_port: u16,

    /// Glass opacity of the player surface, 0-100. Lower is more see-through.
    #[serde(default = "default_transparency")]
    pub transparency: u8,
    /// Backdrop blur strength, 0-100.
    #[serde(default = "default_blur")]
    pub blur: u8,
    /// Solid surface instead of Liquid Glass.
    ///
    /// `false` (default) keeps the frosted material: backdrop blur, album wash,
    /// specular sheen and the cursor light. `true` switches to the flat black
    /// pill of a real iPhone Dynamic Island — one opaque fill, no blur, no
    /// colour bleed from the artwork, no pointer-reactive highlight.
    ///
    /// This is a material choice, not a transparency setting: it deliberately
    /// ignores `transparency` and `blur`, because "solid" means those knobs do
    /// not apply. Keeping it separate from them means turning Solid on cannot
    /// be half-undone by a slider.
    #[serde(default)]
    pub solid_surface: bool,

    #[serde(default)]
    pub mode: WindowMode,

    /// Pointer-hover expansion: idle as mini, touch with pointer to expand to pill;
    /// leave to shrink back. Persisted like any other preference.
    #[serde(default = "default_true", alias = "hoverMini")]
    pub hover_expand: bool,
    #[serde(default = "default_true")]
    pub always_on_top: bool,
    #[serde(default = "default_true")]
    pub start_with_windows: bool,
    #[serde(default = "default_true")]
    pub remember_position: bool,
    #[serde(default = "default_true")]
    pub show_tray: bool,

    /// Last known device volume, 0-100.
    #[serde(default = "default_volume")]
    pub volume: u8,
    /// Device the user explicitly picked, if any.
    #[serde(default)]
    pub device_id: Option<String>,

    // -- Pocket: File Shelf --------------------------------------------------
    /// Master switch for the File Shelf drop target.
    #[serde(default = "default_true")]
    pub shelf_enabled: bool,
    /// Upper bound for a single imported file, in MiB. 0 disables the limit.
    #[serde(default = "default_shelf_max_mib")]
    pub shelf_max_mib: u32,

    // -- Pocket: Clipboard Vault --------------------------------------------
    /// Clipboard capture is opt-in. Off by default: a clipboard history is a
    /// surveillance feature until the user asks for it.
    #[serde(default)]
    pub clipboard_enabled: bool,
    /// Private Mode: stop capturing immediately, keep existing entries.
    #[serde(default)]
    pub clipboard_private: bool,
    /// Maximum stored clipboard entries.
    #[serde(default = "default_clipboard_max_entries")]
    pub clipboard_max_entries: u32,
    /// Retention window in days; older unpinned entries are pruned.
    #[serde(default = "default_clipboard_retention_days")]
    pub clipboard_retention_days: u32,
    /// Process names (lower-case, no extension) whose clipboard content is
    /// never captured — password managers, banking apps, secure terminals.
    #[serde(default)]
    pub clipboard_excluded_apps: Vec<String>,
}

fn default_redirect_port() -> u16 {
    DEFAULT_REDIRECT_PORT
}
fn default_transparency() -> u8 {
    // 50 maps to ~0.30 alpha: clearly translucent over the blurred desktop, and
    // white text still measures about 10:1 contrast (WCAG AAA needs 7:1).
    50
}
fn default_blur() -> u8 {
    72
}
fn default_true() -> bool {
    true
}
fn default_volume() -> u8 {
    70
}
fn default_shelf_max_mib() -> u32 {
    0
}
fn default_clipboard_max_entries() -> u32 {
    100
}
fn default_clipboard_retention_days() -> u32 {
    7
}

/// Default exclusion list for clipboard capture: the apps whose clipboard
/// content is most likely to be a credential.
///
/// Deliberately empty rather than pre-filled with guesses: silently dropping
/// clipboard content based on a process name is worse than asking the user. The
/// setting exists so the user can name their password manager themselves.
fn default_excluded_apps() -> Vec<String> {
    Vec::new()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            client_id: String::new(),
            redirect_port: DEFAULT_REDIRECT_PORT,
            transparency: default_transparency(),
            blur: default_blur(),
            solid_surface: false,
            mode: WindowMode::Mini,
            hover_expand: true,
            always_on_top: true,
            start_with_windows: true,
            remember_position: true,
            show_tray: true,
            volume: default_volume(),
            device_id: None,
            shelf_enabled: true,
            shelf_max_mib: default_shelf_max_mib(),
            clipboard_enabled: false,
            clipboard_private: false,
            clipboard_max_entries: default_clipboard_max_entries(),
            clipboard_retention_days: default_clipboard_retention_days(),
            clipboard_excluded_apps: default_excluded_apps(),
        }
    }
}

/// Where the player remembers its top-left corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Geometry {
    pub x: i32,
    pub y: i32,
}

/// Per-user application data directory: `%APPDATA%\com.sonic.player`.
pub fn app_dir() -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        let dir = PathBuf::from(appdata).join("com.sonic.player");
        let _ = fs::create_dir_all(&dir);
        return dir;
    }
    // Extremely defensive fallback; never expected on Windows.
    let dir = std::env::temp_dir().join("com.sonic.player");
    let _ = fs::create_dir_all(&dir);
    dir
}

pub fn settings_path() -> PathBuf {
    app_dir().join("settings.json")
}

pub fn geometry_path() -> PathBuf {
    app_dir().join("geometry.json")
}

/// Directory the File Shelf copies user files into.
///
/// Deliberately a directory of real files, not a blob column: Windows Explorer
/// has to be able to open them, and a copy is the whole point.
pub fn shelf_dir() -> PathBuf {
    app_dir().join("shelf")
}

/// Where the File Shelf keeps its index.
pub fn shelf_index_path() -> PathBuf {
    app_dir().join("shelf.json")
}

/// Directory the Clipboard Vault stores captured images in.
pub fn clipboard_dir() -> PathBuf {
    app_dir().join("clipboard")
}

/// Where the Clipboard Vault keeps its index.
pub fn clipboard_index_path() -> PathBuf {
    app_dir().join("clipboard.json")
}

/// Disk usage of Pocket, for the settings readout.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PocketUsage {
    pub shelf_count: u32,
    pub shelf_bytes: u64,
    pub clipboard_count: u32,
    pub clipboard_bytes: u64,
    pub shelf_path: String,
    pub clipboard_path: String,
}

/// Loads `SONIC_*` variables from a `.env` file next to the executable, without
/// adding a dependency.
///
/// `.env.example` tells people to copy it to `.env`, so not reading it made that
/// instruction a lie: the file existed and nothing ever looked at it. Values that
/// are already present in the real environment win, which keeps shell exports and
/// CI overrides authoritative.
fn load_dotenv() {
    use std::sync::Once;

    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        // Next to the exe in a release build; the project root in `tauri dev`.
        let mut candidates = Vec::new();
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                candidates.push(dir.join(".env"));
                // dev: <root>/src-tauri/target/debug/.env -> <root>/src-tauri/.env
                if let Some(srctauri) = dir.parent() {
                    candidates.push(srctauri.join(".env"));
                    if let Some(root) = srctauri.parent() {
                        candidates.push(root.join(".env"));
                    }
                }
            }
        }
        if let Ok(cwd) = std::env::current_dir() {
            candidates.push(cwd.join(".env"));
        }

        for path in candidates {
            let Ok(raw) = fs::read_to_string(&path) else {
                continue;
            };
            for line in raw.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                let line = line.strip_prefix("export ").unwrap_or(line);
                let Some((key, value)) = line.split_once('=') else {
                    continue;
                };
                let key = key.trim();
                if key.is_empty() || std::env::var_os(key).is_some() {
                    continue;
                }
                let value = value.trim();
                // Strip one layer of matching quotes so `A="x y"` yields `x y`.
                let value = if value.len() >= 2 {
                    let bytes = value.as_bytes();
                    let first = bytes[0];
                    let last = bytes[bytes.len() - 1];
                    if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
                        &value[1..value.len() - 1]
                    } else {
                        value
                    }
                } else {
                    value
                };
                std::env::set_var(key, value);
            }
            break;
        }
    });
}

/// Reads settings, layering the environment override on top.
pub fn load_settings() -> Settings {
    load_dotenv();

    let mut settings: Settings = fs::read_to_string(settings_path())
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default();

    if let Ok(id) = std::env::var(CLIENT_ID_ENV) {
        let id = id.trim().to_string();
        if !id.is_empty() {
            settings.client_id = id;
        }
    }
    if let Ok(port) = std::env::var(REDIRECT_PORT_ENV) {
        if let Ok(port) = port.trim().parse::<u16>() {
            if port > 0 {
                settings.redirect_port = port;
            }
        }
    }

    settings
}

/// Writes settings atomically so a crash can never leave a truncated file.
pub fn save_settings(settings: &Settings) -> std::io::Result<()> {
    let path = settings_path();
    let json = serde_json::to_string_pretty(settings)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json)?;
    fs::rename(&tmp, &path)
}

pub fn load_geometry() -> Option<Geometry> {
    fs::read_to_string(geometry_path())
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
}

pub fn save_geometry(geometry: Geometry) -> std::io::Result<()> {
    let path = geometry_path();
    let json = serde_json::to_string_pretty(&geometry)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    fs::write(path, json)
}

/// Full redirect URI for the current settings.
pub fn redirect_uri(port: u16) -> String {
    format!("http://127.0.0.1:{port}{REDIRECT_PATH}")
}

// ---------------------------------------------------------------------------
// Autostart (HKCU ...\CurrentVersion\Run)
// ---------------------------------------------------------------------------

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "Sonic";

static AUTOSTART_AVAILABLE: AtomicBool = AtomicBool::new(true);

/// Registers (or removes) the per-user Run key entry.
///
/// Only the current user is touched: no elevation, no machine-wide changes.
pub fn set_autostart(enabled: bool) -> Result<(), String> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu
        .create_subkey(RUN_KEY)
        .map_err(|e| format!("cannot open Run key: {e}"))?;

    if enabled {
        let exe =
            std::env::current_exe().map_err(|e| format!("cannot resolve executable path: {e}"))?;
        // Quote the path so spaces in the user's profile do not split argv.
        let command = format!("\"{}\"", exe.display());
        key.set_value(RUN_VALUE, &command)
            .map_err(|e| format!("cannot write Run value: {e}"))?;
        AUTOSTART_AVAILABLE.store(true, Ordering::Relaxed);
    } else {
        // Deleting a missing value is not an error worth surfacing.
        let _ = key.delete_value(RUN_VALUE);
    }
    Ok(())
}

/// Whether the Run key currently points at this executable.
pub fn autostart_enabled() -> bool {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let Ok(hkcu) = RegKey::predef(HKEY_CURRENT_USER).open_subkey(RUN_KEY) else {
        return false;
    };
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let Ok(value) = hkcu.get_value::<String, _>(RUN_VALUE) else {
        return false;
    };
    let expected = format!("\"{}\"", exe.display());
    if value == expected {
        return true;
    }
    // Tolerate a legacy unquoted entry written by an older build.
    value.trim() == exe.display().to_string()
}

pub fn autostart_supported() -> bool {
    AUTOSTART_AVAILABLE.load(Ordering::Relaxed)
}
