/**
 * Types mirrored from the Rust backend (`src-tauri/src/spotify.rs`, `state.rs`,
 * `config.rs`, `commands.rs`).
 *
 * These are the exact shapes Sonic receives over Tauri commands and events.
 * Nothing here is fetched from Spotify directly — the frontend never holds a
 * token.
 */

export type WindowMode = "pill" | "expanded" | "mini";

export type RepeatState = "off" | "track" | "context";

export interface Device {
  id: string;
  name: string;
  type: string;
  isActive: boolean;
  isRestricted: boolean;
  volumePercent: number;
  supportsVolume: boolean;
}

export interface TrackItem {
  id: string;
  uri: string;
  name: string;
  artists: string[];
  album: string;
  albumId: string;
  artworkUrl: string;
  artworkSmallUrl: string;
  durationMs: number;
  explicit: boolean;
  kind: string;
  externalUrl: string;
}

export interface Playback {
  isPlaying: boolean;
  progressMs: number;
  device: Device;
  item: TrackItem | null;
  shuffleState: boolean;
  repeatState: RepeatState;
}

/** Capabilities mirrored from the local Windows media session. */
export interface LocalControls {
  canPlay: boolean;
  canPause: boolean;
  canNext: boolean;
  canPrevious: boolean;
  canSeek: boolean;
  canShuffle: boolean;
  canRepeat: boolean;
}

export interface UiSettings {
  /** Pointer resting on mini expands to pill; leaving restores mini. */
  hoverExpand?: boolean;
  hoverMini?: boolean;
  transparency: number;
  blur: number;
  /** True = flat opaque pill (iPhone Dynamic Island), false = Liquid Glass. */
  solidSurface?: boolean;
  mode: WindowMode;
  alwaysOnTop: boolean;
  startWithWindows: boolean;
  rememberPosition: boolean;
  showTray: boolean;
  volume: number;
  hasClientId: boolean;
  redirectUri: string;

  // -- Pocket: File Shelf ---------------------------------------------------
  shelfEnabled: boolean;
  shelfMaxMib: number;
  shelfCount: number;
  shelfBytes: number;

  // -- Pocket: Clipboard Vault ---------------------------------------------
  clipboardEnabled: boolean;
  clipboardPrivate: boolean;
  clipboardMaxEntries: number;
  clipboardRetentionDays: number;
  clipboardExcludedApps: string[];
  clipboardCount: number;
  clipboardBytes: number;
  /** Bumped on every capture, so the vault list can refresh itself. */
  clipboardRevision: number;
  /** Live clipboard sequence number; moves whenever Windows' clipboard changes. */
  clipboardSequence: number;
  /** True while a native file drag is over the player. */
  dropTarget: boolean;
}

/** How a stored file is categorised for display. */
export type ShelfKind =
  | "image"
  | "audio"
  | "video"
  | "document"
  | "archive"
  | "other";

export interface ShelfEntry {
  id: string;
  /** The name the user dropped, kept verbatim. */
  name: string;
  /** Absolute path of Sonic's copy — the original is never touched. */
  path: string;
  size: number;
  ext: string;
  kind: ShelfKind;
  /** Unix seconds. */
  addedAt: number;
  pinned: boolean;
}

export interface SkippedFile {
  name: string;
  reason: string;
}

export interface ImportReport {
  added: ShelfEntry[];
  skipped: SkippedFile[];
}

export type ClipboardKind = "text" | "image";

export interface ClipboardEntry {
  id: string;
  kind: ClipboardKind;
  text: string | null;
  imagePath: string | null;
  width: number | null;
  height: number | null;
  /** Unix seconds. */
  capturedAt: number;
  /** The app that owned the clipboard, e.g. `firefox`. */
  source: string;
  pinned: boolean;
}

export interface PocketUsage {
  shelfCount: number;
  shelfBytes: number;
  clipboardCount: number;
  clipboardBytes: number;
  shelfPath: string;
  clipboardPath: string;
}

/** Machine-readable error codes emitted by the backend. */
export type ErrorCode =
  | "unauthorized"
  | "forbidden"
  | "no_active_device"
  | "rate_limited"
  | "not_found"
  | "network"
  | "refresh_failed"
  | "local";

export interface AppError {
  code: ErrorCode;
  message: string;
  retryAfterSeconds: number | null;
}

export interface UiState {
  connected: boolean;
  authenticating: boolean;
  accountLabel: string;
  accountName: string;
  avatarUrl: string;
  isPremium: boolean;
  error: AppError | null;
  devices: Device[];
  playback: Playback | null;
  settings: UiSettings;
  ready: boolean;
  localMediaAvailable: boolean;
  localMediaPlaying: boolean;
  localMediaTitle: string | null;
  localMediaArtist: string | null;
  localMediaArtworkDataUrl: string | null;
  /** Local timeline, so the bar and time labels work without Spotify. */
  localMediaPositionMs: number | null;
  localMediaDurationMs: number | null;
  localMediaShuffle: boolean | null;
  localMediaRepeat: string | null;
  localMediaControls: LocalControls;
  /** Transient hover-expansion state, owned by Rust. */
  hoverExpand?: boolean;
  hoverMini?: boolean;
  /**
   * The mode to actually draw. `settings.mode` is the user's intent; this is
   * what the window currently is. They diverge only during hover expansion.
   */
  effectiveMode: WindowMode;
}

export interface Bootstrap {
  state: UiState;
  clientIdPresent: boolean;
  redirectUri: string;
  platform: string;
  autostartSupported: boolean;
}

export interface Geometry {
  x: number;
  y: number;
}

export type Transport = "play" | "pause" | "toggle" | "next" | "previous";

/** Event names the backend emits. */
export const EVENTS = {
  state: "sonic://state",
  authRequired: "sonic://auth-required",
  openSettings: "sonic://open-settings",
} as const;