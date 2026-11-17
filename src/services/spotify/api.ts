import { invoke } from "@tauri-apps/api/core";
import type {
  Bootstrap,
  ClipboardEntry,
  ImportReport,
  PocketUsage,
  ShelfEntry,
  Transport,
  UiSettings,
  WindowMode,
} from "../../types/spotify";

/**
 * Every Spotify operation is a Rust command.
 *
 * The frontend deliberately has no `fetch` to Spotify: tokens live in the
 * Windows Credential Manager and are only ever used inside the Rust process.
 */
export const api = {
  bootstrap: () => invoke<Bootstrap>("get_bootstrap"),

  startLogin: () => invoke<void>("start_login"),
  cancelLogin: () => invoke<void>("cancel_login"),
  logout: () => invoke<void>("logout"),

  selectDevice: (deviceId: string, play = true) =>
    invoke<void>("select_device", { deviceId, play }),

  transport: (command: Transport, deviceId?: string) =>
    invoke<void>("playback_command", { command, deviceId: deviceId ?? null }),

  seek: (positionMs: number, deviceId?: string) =>
    invoke<void>("seek", { positionMs, deviceId: deviceId ?? null }),

  setVolume: (volumePercent: number, deviceId?: string) =>
    invoke<void>("set_volume", {
      volumePercent,
      deviceId: deviceId ?? null,
    }),

  setShuffle: (state: boolean, deviceId?: string) =>
    invoke<void>("set_shuffle", { state, deviceId: deviceId ?? null }),

  setRepeat: (repeatState: string, deviceId?: string) =>
    invoke<void>("set_repeat", { repeatState, deviceId: deviceId ?? null }),

  openSpotify: (url?: string) => invoke<void>("open_spotify", { url: url ?? null }),

  updateSettings: (patch: Record<string, unknown>) =>
    invoke<UiSettings>("update_settings", { patch }),

  setWindowMode: (mode: WindowMode) => invoke<WindowMode>("set_window_mode", { mode }),
  setAlwaysOnTop: (enabled: boolean) => invoke<void>("set_always_on_top", { enabled }),

  /** Settings is not a player mode, so its height is requested explicitly. */
  setSettingsOpen: (open: boolean) => invoke<void>("set_settings_open", { open }),

  /** Pocket is not a player mode either: its own height, its own panel. */
  setPocketOpen: (open: boolean) => invoke<void>("set_pocket_open", { open }),

  /** Tells Rust a native file drag entered or left the player. */
  setDropTarget: (active: boolean) => invoke<void>("set_drop_target", { active }),

  /**
   * Tells Rust that a drag or a popover owns the pointer, so the hover watcher
   * stands down and cannot shrink the window mid-gesture.
   */
  setPointerClaimed: (claimed: boolean) =>
    invoke<void>("set_pointer_claimed", { claimed }),

  hideWindow: () => invoke<void>("hide_window"),
  quit: () => invoke<void>("quit_app"),

  // -- Pocket: File Shelf ---------------------------------------------------

  /** Copies absolute paths into the shelf. Originals are never modified. */
  shelfImport: (paths: string[]) =>
    invoke<ImportReport>("shelf_import", { paths }),

  shelfList: () => invoke<ShelfEntry[]>("shelf_list"),

  shelfSetPinned: (id: string, pinned: boolean) =>
    invoke<void>("shelf_set_pinned", { id, pinned }),

  /** Deletes Sonic's stored copy, not the original file. */
  shelfRemove: (id: string) => invoke<void>("shelf_remove", { id }),

  shelfClear: () => invoke<number>("shelf_clear"),

  shelfOpen: (path: string) => invoke<void>("shelf_open", { path }),

  shelfReveal: (path: string) => invoke<void>("shelf_reveal", { path }),

  /**
   * Starts a real OLE drag of Sonic's stored copies.
   *
   * Resolves only when the user drops or cancels, because the Windows modal drag
   * loop owns the process until then. `ghost` is an optional PNG data URL used
   * as the drag image.
   */
  shelfDragOut: (paths: string[], ghost?: string) =>
    invoke<void>("shelf_drag_out", { paths, ghost: ghost ?? null }),

  // -- Pocket: Clipboard Vault ---------------------------------------------

  clipboardList: () => invoke<ClipboardEntry[]>("clipboard_list"),

  /** Puts an entry back on the real Windows clipboard so Ctrl+V works. */
  clipboardRestore: (id: string) => invoke<void>("clipboard_restore", { id }),

  clipboardSetPinned: (id: string, pinned: boolean) =>
    invoke<void>("clipboard_set_pinned", { id, pinned }),

  clipboardRemove: (id: string) => invoke<void>("clipboard_remove", { id }),

  clipboardClear: () => invoke<number>("clipboard_clear"),

  /** Forces a capture pass now, for the empty state's button. */
  clipboardCaptureNow: () => invoke<boolean>("clipboard_capture_now"),

  /** Runs retention cleanup and deletes orphaned images. */
  clipboardPrune: () => invoke<number>("clipboard_prune"),

  pocketUsage: () => invoke<PocketUsage>("pocket_usage"),
};

// `showWindow`, `currentPlayback` and `saveGeometry` were removed rather than
// wired up: the first is reachable through the tray, and the other two called
// commands that were never registered in `generate_handler!`, so any call would
// have failed with "command not found".